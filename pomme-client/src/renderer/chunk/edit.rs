//! Bounded immediate terrain replacement, never a synchronous section remesh.
use std::collections::HashMap;

use azalea_block::BlockState;
use azalea_core::position::{BlockPos, ChunkPos};

use super::mesher::SectionMesh;

pub const MAX_EDIT_CELLS: usize = 64;
pub const MAX_EDIT_VERTICES: usize = 8192;
pub const MAX_EDIT_INDICES: usize = 12288;

pub struct EditCellMesh {
    pub pos: BlockPos,
    pub state: BlockState,
    pub origin: [i32; 3],
    pub mesh: SectionMesh,
}

pub(crate) struct PendingCell {
    pub geometry: EditCellMesh,
    pub expected_generation: Option<u64>,
}

#[derive(Default)]
pub(crate) struct EditOverlay {
    pub cells: HashMap<BlockPos, PendingCell>,
    pub recorder: Option<std::sync::Arc<crate::movement_record::Recorder>>,
}

// ponytail: only face-adjacent cells are redrawn; diagonal AO/light changes
// wait for the async mesh. Expand to 27 cells if that temporary seam matters.
pub fn edit_neighborhood(pos: BlockPos) -> [BlockPos; 7] {
    [
        pos,
        BlockPos::new(pos.x.saturating_sub(1), pos.y, pos.z),
        BlockPos::new(pos.x.saturating_add(1), pos.y, pos.z),
        BlockPos::new(pos.x, pos.y.saturating_sub(1), pos.z),
        BlockPos::new(pos.x, pos.y.saturating_add(1), pos.z),
        BlockPos::new(pos.x, pos.y, pos.z.saturating_sub(1)),
        BlockPos::new(pos.x, pos.y, pos.z.saturating_add(1)),
    ]
}

impl EditOverlay {
    /// Stage only the affected cells; rejection must not erase other previews.
    pub fn admit(
        &mut self,
        dispatcher: &super::mesher::MeshDispatcher,
        chunks: &crate::world::chunk::ChunkStore,
        animations: &crate::world::block_entity_anim::BlockEntityAnimStore,
        pos: BlockPos,
        previous: BlockState,
    ) -> Result<(), &'static str> {
        let affected = edit_neighborhood(pos);
        let candidate = (|| {
            if !dispatcher.supports_immediate_edit(previous, pos) {
                return Err("unsupported_previous");
            }
            // Fail closed for partial neighbours: their old geometry has no
            // proven source-voxel ownership, even inside an adjacent cube.
            if affected.iter().any(|p| {
                !dispatcher.supports_immediate_edit(chunks.get_block_state(p.x, p.y, p.z), *p)
            }) {
                return Err("unsupported_current_or_neighbor");
            }
            let meshes = dispatcher
                .mesh_edit_cells(chunks, animations, &affected)
                .ok_or("geometry_cap")?;
            let retained = self
                .cells
                .values()
                .filter(|c| !affected.contains(&c.geometry.pos));
            let (mut count, mut vertices, mut indices) = (meshes.len(), 0, 0);
            for geometry in retained.map(|c| &c.geometry).chain(meshes.iter()) {
                if !affected.contains(&geometry.pos) {
                    count += 1;
                }
                vertices += geometry.mesh.vertices.len();
                indices += geometry.mesh.indices.len();
            }
            if count > MAX_EDIT_CELLS {
                return Err("cell_cap");
            }
            if vertices > MAX_EDIT_VERTICES {
                return Err("vertex_cap");
            }
            if indices > MAX_EDIT_INDICES {
                return Err("index_cap");
            }
            Ok(meshes)
        })();
        let result = match candidate {
            Ok(meshes) => {
                for geometry in meshes {
                    let expected_generation = self
                        .cells
                        .get(&geometry.pos)
                        .and_then(|c| c.expected_generation);
                    self.cells.insert(
                        geometry.pos,
                        PendingCell {
                            geometry,
                            expected_generation,
                        },
                    );
                }
                Ok(())
            }
            Err(reason) => {
                // The world already changed: never leave an obsolete same-pos
                // delta or faces built against it. Only this influence set falls back.
                self.cells.retain(|p, _| !affected.contains(p));
                Err(reason)
            }
        };
        if let Some(recorder) = &self.recorder {
            recorder.record("local", "visual_edit_admission", || Some(serde_json::json!({
                "pos":[pos.x,pos.y,pos.z],"before":previous.id(),
                "state":chunks.get_block_state(pos.x,pos.y,pos.z).id(),
                "success":result.is_ok(),"reason":result.err(),"pending_cells":self.cells.len(),
                "caps":{"cells":MAX_EDIT_CELLS,"vertices":MAX_EDIT_VERTICES,"indices":MAX_EDIT_INDICES}
            })));
        }
        result
    }

    #[cfg(test)]
    pub fn replace(&mut self, meshes: Vec<EditCellMesh>) {
        let old = std::mem::take(&mut self.cells);
        self.cells = meshes
            .into_iter()
            .map(|geometry| {
                let expected_generation =
                    old.get(&geometry.pos).and_then(|c| c.expected_generation);
                (
                    geometry.pos,
                    PendingCell {
                        geometry,
                        expected_generation,
                    },
                )
            })
            .collect();
    }

    /// Call after EVERY priority enqueue (including stale-result retries).
    pub fn expect(&mut self, col: ChunkPos, range: std::ops::Range<i32>, generation: u64) {
        for cell in self.cells.values_mut() {
            let p = cell.geometry.pos;
            if ChunkPos::new(p.x.div_euclid(16), p.z.div_euclid(16)) == col
                && range.contains(&cell.geometry.mesh.section_index)
            {
                cell.expected_generation = Some(generation);
            }
        }
    }

    /// Only an accepted GPU replacement in the edit-generation domain retires
    /// a cell. An ACK, stale result, bulk mesh or failed allocation cannot.
    pub fn uploaded(
        &mut self,
        col: ChunkPos,
        sections: &std::collections::HashSet<i32>,
        generation: u64,
        column_revision: u64,
        upload_epoch: u64,
    ) {
        self.cells.retain(|_, cell| {
            let p = cell.geometry.pos;
            let retire = ChunkPos::new(p.x.div_euclid(16), p.z.div_euclid(16)) == col
                && sections.contains(&cell.geometry.mesh.section_index)
                && cell.expected_generation == Some(generation);
            if let Some(recorder) = &self.recorder {
                recorder.record("local", "visual_edit_retire", || Some(serde_json::json!({
                    "pos":[p.x,p.y,p.z],"generation":generation,"expected_generation":cell.expected_generation,
                    "column_revision":column_revision,"upload_epoch":upload_epoch,"accepted":retire
                })));
            }
            !retire
        });
    }
}

/// CPU mirror of the shader rule. The epsilon exceeds packed-position error;
/// tangential coordinates stay unmodified so only one cell of a greedy face
/// disappears. Supported geometry is restricted to unit-cube boundary faces.
#[cfg(test)]
fn source_voxel(local: [f32; 3], normal: [f32; 3], origin: [i32; 3]) -> [i32; 3] {
    std::array::from_fn(|i| (local[i] - normal[i] * 0.001).floor() as i32 + origin[i])
}

#[cfg(test)]
mod tests {
    use super::super::mesher::ChunkAABB;
    use super::*;

    fn geometry(pos: BlockPos, state: BlockState) -> EditCellMesh {
        EditCellMesh {
            pos,
            state,
            origin: [-16, -64, -16],
            mesh: SectionMesh {
                section_index: (pos.y + 64).div_euclid(16),
                vertices: Vec::new(),
                aabb: ChunkAABB {
                    min: [0.0; 4],
                    max: [1.0; 4],
                },
                indices: Vec::new(),
                solid_index_count: 0,
                water_indices: Vec::new(),
                translucent_indices: Vec::new(),
                emitted_chests: Vec::new(),
                trace: Vec::new(),
            },
        }
    }

    #[test]
    fn immediate_edit_source_voxel_greedy_faces_negative_boundary() {
        for (point, normal, expected) in [
            ([16.0001, 3.25, 4.5], [1.0, 0.0, 0.0], [-1, -61, -12]),
            ([-0.0001, 3.25, 4.5], [-1.0, 0.0, 0.0], [-16, -61, -12]),
            ([2.5, 16.0001, 4.5], [0.0, 1.0, 0.0], [-14, -49, -12]),
            ([2.5, -0.0001, 4.5], [0.0, -1.0, 0.0], [-14, -64, -12]),
            ([2.5, 3.25, 16.0001], [0.0, 0.0, 1.0], [-14, -61, -1]),
            ([2.5, 3.25, -0.0001], [0.0, 0.0, -1.0], [-14, -61, -16]),
        ] {
            assert_eq!(source_voxel(point, normal, [-16, -64, -16]), expected);
        }
        let edited = [-14, -61, -12];
        for x in 0..16 {
            assert_eq!(
                source_voxel(
                    [x as f32 + 0.5, 4.0001, 4.5],
                    [0.0, 1.0, 0.0],
                    [-16, -64, -16]
                ) == edited,
                x == 2
            );
        }
        let p = BlockPos::new(-1, -49, -1);
        assert_eq!(edit_neighborhood(p).len(), 7);
        assert!(edit_neighborhood(p).contains(&BlockPos::new(0, -49, -1)));
        assert!(edit_neighborhood(p).contains(&BlockPos::new(-1, -48, -1)));
    }

    #[test]
    fn immediate_edit_latest_overwrite_and_exact_uploaded_generation() {
        let _protocol = crate::world::block::test_protocol_guard();
        // Section indices are relative to min_y=-64, not world section Y.
        // -49 is section 0; -48 is the first cell of section 1.
        let p = BlockPos::new(-1, -48, -1);
        assert_eq!(geometry(p, BlockState::AIR).mesh.section_index, 1);
        assert_eq!(
            geometry(BlockPos::new(-1, -49, -1), BlockState::AIR)
                .mesh
                .section_index,
            0
        );
        let col = ChunkPos::new(-1, -1);
        let mut overlay = EditOverlay::default();
        overlay.replace(vec![geometry(p, BlockState::AIR)]);
        overlay.expect(col, 0..2, 10);
        crate::world::block::init("26.2");
        let stone = crate::world::block::first_state_of("stone").unwrap();
        let latest = geometry(p, stone);
        overlay.replace(vec![latest]);
        overlay.expect(col, 0..2, 11);
        overlay.uploaded(col, &std::collections::HashSet::from([0, 1]), 10, 0, 10);
        assert_eq!(overlay.cells[&p].geometry.state, stone);
        overlay.uploaded(col, &std::collections::HashSet::from([0]), 11, 0, 11);
        assert_eq!(overlay.cells.len(), 1); // wrong section
        overlay.uploaded(col, &std::collections::HashSet::from([1]), 11, 0, 11);
        assert!(overlay.cells.is_empty()); // accepted empty section also retires
    }
}
