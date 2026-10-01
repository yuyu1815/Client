package com.mine_rust.movementobserver;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.Property;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.minecraft.world.phys.shapes.CollisionContext;
import net.minecraft.world.phys.shapes.VoxelShape;

import java.util.ArrayList;
import java.util.List;

/** Bounded current-world requery; never evidence of the actual resolver inputs. */
final class CollisionSnapshots {
    static final int MAX_CELLS = 32, MAX_BLOCK_BOXES = 128, MAX_ENTITY_BOXES = 8;
    private CollisionSnapshots() {}

    static JsonObject capture(LocalPlayer player, AABB bbox, Vec3 requestedDelta) {
        JsonObject out = new JsonObject();
        AABB region = bbox.expandTowards(requestedDelta);
        out.addProperty("semantics", "current-world requery; NOT actual collision inputs; truncated reports cap omissions only, not world completeness");
        out.add("region", box(region));
        JsonArray cells = new JsonArray(), boxes = new JsonArray();
        Level level = player == null ? null : player.level();
        int visited = 0, omittedBoxes = 0, unloaded = 0, outsideHeight = 0, failed = 0;
        long omittedCells = 0;
        int minX = floor(region.minX), minY = floor(region.minY) - 1, minZ = floor(region.minZ);
        int maxX = ceil(region.maxX), maxY = ceil(region.maxY), maxZ = ceil(region.maxZ);
        long total = saturatedMultiply(saturatedMultiply(extent(minX, maxX), extent(minY, maxY)), extent(minZ, maxZ));
        JsonObject range = new JsonObject(); range.add("min", xyz(minX, minY, minZ)); range.add("max_exclusive", xyz(maxX, maxY, maxZ));
        out.add("visited_range", range);
        out.addProperty("visited_range_key", "y,z,x-order prefix of "+minX+":"+maxX+","+minY+":"+maxY+","+minZ+":"+maxZ+"; visited="+(level == null ? 0 : Math.min(total, MAX_CELLS)));
        if (level == null) {
            out.addProperty("status", "null_world");
            omittedCells = total;
        } else {
            out.addProperty("status", "ok");
            BlockPos.MutableBlockPos pos = new BlockPos.MutableBlockPos();
            outer: for (int y = minY; y < maxY; y++) for (int z = minZ; z < maxZ; z++) for (int x = minX; x < maxX; x++) {
                if (visited == MAX_CELLS) break outer;
                visited++;
                pos.set(x, y, z);
                if (level.isOutsideBuildHeight(pos)) { outsideHeight++; continue; }
                try {
                    if (!level.isLoaded(pos)) { unloaded++; continue; }
                    BlockState state = level.getBlockState(pos);
                    JsonObject cell = new JsonObject(); cell.add("pos", xyz(x, y, z));
                    cell.addProperty("state_id", net.minecraft.world.level.block.Block.BLOCK_STATE_REGISTRY.getId(state));
                    var name = BuiltInRegistries.BLOCK.getKey(state.getBlock());
                    cell.addProperty("name", name == null ? "unknown" : name.toString());
                    JsonObject properties = new JsonObject();
                    state.getValues().sorted(java.util.Comparator.comparing(v -> propertyName(v.property())))
                            .forEach(v -> properties.addProperty(propertyName(v.property()), propertyValue(v.property(), v.value())));
                    cell.add("properties", properties); cells.add(cell);
                    List<AABB> shapeBoxes = state.getCollisionShape(level, pos, CollisionContext.of(player)).toAabbs();
                    List<AABB> kept = boundedBoxes(shapeBoxes, MAX_BLOCK_BOXES - boxes.size());
                    omittedBoxes += shapeBoxes.size() - kept.size();
                    for (AABB localOrWorld : kept) {
                        JsonObject b = new JsonObject(); b.add("block", xyz(x, y, z));
                        b.add("aabb", box(worldBox(localOrWorld, x, y, z))); boxes.add(b);
                    }
                } catch (RuntimeException ex) { failed++; }
            }
            omittedCells = Math.max(0, total - visited);
            if (failed > 0) out.addProperty("status", "requery_failed");
        }
        out.add("block_cells", cells); out.add("shape_aabbs", boxes);
        out.addProperty("max_block_cells", MAX_CELLS); out.addProperty("max_boxes", MAX_BLOCK_BOXES);
        out.addProperty("visited_cells", visited); out.addProperty("omitted_block_cells", omittedCells);
        out.addProperty("omitted_boxes_in_visited_cells", omittedBoxes);
        out.addProperty("unloaded_cells", unloaded); out.addProperty("outside_height_cells", outsideHeight);
        out.addProperty("failed_cells", failed); out.addProperty("unresolved_cells", unloaded + failed); out.addProperty("boxes_in_unvisited_cells", (String)null);
        out.addProperty("truncated", omittedCells > 0 || omittedBoxes > 0);
        JsonArray entityBoxes = new JsonArray(); int entityOmitted = 0; String entityStatus = "ok";
        if (level == null) entityStatus = "null_world";
        else try {
            AABB swept = bbox.expandTowards(requestedDelta);
            List<VoxelShape> shapes = level.getEntityCollisions(player, swept);
            for (VoxelShape shape : shapes) for (AABB b : shape.toAabbs()) {
                if (!finite(b) || entityBoxes.size() >= MAX_ENTITY_BOXES) entityOmitted++;
                else entityBoxes.add(box(b));
            }
        } catch (RuntimeException ex) { entityStatus = "requery_failed"; }
        out.add("entity_shapes", entityBoxes); out.addProperty("entity_shapes_semantics", "current-world requery; NOT actual collision inputs"); out.addProperty("entity_shapes_max", MAX_ENTITY_BOXES);
        out.addProperty("entity_shapes_omitted", entityOmitted); out.addProperty("entity_shapes_status", entityStatus);
        JsonArray border = new JsonArray(); int borderOmitted = 0; String borderStatus = "ok";
        if (level == null) borderStatus = "null_world";
        else try {
            List<AABB> candidates=level.getWorldBorder().getCollisionShape().toAabbs();
            List<AABB> kept=boundedBoxes(candidates,MAX_ENTITY_BOXES);
            for (AABB b : kept) border.add(box(b));
            borderOmitted=candidates.size()-kept.size();
        } catch (RuntimeException ex) { borderStatus = "requery_failed"; }
        out.add("world_border_shapes", border); out.addProperty("world_border_shapes_semantics", "current-world border collision shape requery; NOT actual collision inputs"); out.addProperty("world_border_shapes_max", MAX_ENTITY_BOXES);
        out.addProperty("world_border_shapes_omitted", borderOmitted); out.addProperty("world_border_status", borderStatus);
        if (level == null) out.add("current_border_bounds", null);
        else { var worldBorder=level.getWorldBorder(); JsonArray bounds=new JsonArray(); bounds.add(worldBorder.getMinX()); bounds.add(worldBorder.getMaxX()); bounds.add(worldBorder.getMinZ()); bounds.add(worldBorder.getMaxZ()); out.add("current_border_bounds", bounds); }
        return out;
    }

    static AABB worldBox(AABB shapeBox, int x, int y, int z) { return shapeBox.move(x, y, z); }
    static List<AABB> boundedBoxes(List<AABB> shapes, int cap) {
        List<AABB> out=new ArrayList<>(Math.min(Math.max(0,cap),shapes.size()));
        for (AABB b : shapes) if (finite(b) && out.size()<cap) out.add(b);
        return out;
    }
    static boolean finite(AABB b) {
        return Double.isFinite(b.minX) && Double.isFinite(b.minY) && Double.isFinite(b.minZ)
                && Double.isFinite(b.maxX) && Double.isFinite(b.maxY) && Double.isFinite(b.maxZ);
    }
    static JsonArray box(AABB b) { JsonArray a=new JsonArray(); a.add(xyz(b.minX,b.minY,b.minZ)); a.add(xyz(b.maxX,b.maxY,b.maxZ)); return a; }
    static JsonArray xyz(double x,double y,double z) { JsonArray a=new JsonArray();a.add(x);a.add(y);a.add(z);return a; }
    private static String propertyName(Property<?> p) { return p.getName(); }
    private static <T extends Comparable<T>> String propertyValue(Property<T> p, Object value) { return p.getName(p.getValueClass().cast(value)); }
    private static int floor(double d) { return (int)Math.floor(d); }
    private static int ceil(double d) { return (int)Math.ceil(d); }
    private static long extent(int a,int b) { return Math.max(0L,(long)b-a); }
    private static long saturatedMultiply(long a,long b) { return a == 0 || b == 0 ? 0 : a > Long.MAX_VALUE / b ? Long.MAX_VALUE : a*b; }
}
