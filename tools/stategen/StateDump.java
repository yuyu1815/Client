import java.io.BufferedWriter;
import java.io.IOException;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Dumps per-block-state properties (light, collision, water replacement, and
 * LiquidBlockContainer behavior) by running vanilla's own code: bootstraps the
 * block registry from the server jar on the classpath, then iterates
 * Block.BLOCK_STATE_REGISTRY in state-id order.
 *
 * Everything is reflection so one binary covers the 26.x API
 * (getLightDampening), the 1.21.2+ API (getLightBlock), and the older
 * world-context API (getLightBlock(BlockGetter, BlockPos), fed the empty
 * getter exactly like vanilla's own state cache); it also means the tool
 * compiles against nothing but the JDK.
 *
 * Face-occlusion shapes are emitted as 16x16 bitmasks over the face plane.
 * Vanilla's faceShapeOccludes(a, b) tests whether the union of two face
 * shapes covers the full block; since face shapes span their slice axis,
 * that reduces to 2D coverage — exact as long as every shape is 1/16-aligned,
 * which this tool hard-fails on if violated.
 *
 * Usage: java -cp <server-classes-jar>;<bundled-libs...>;. StateDump <version> <out.json>
 */
public final class StateDump {
    // Direction.values() order (DOWN, UP, NORTH, SOUTH, WEST, EAST) -> slice axis:
    // Y for down/up, Z for north/south, X for west/east.
    private static final char[] AXIS_BY_ORDINAL = {'Y', 'Y', 'Z', 'Z', 'X', 'X'};

    public static void main(String[] args) throws Exception {
        if (args.length != 2) {
            System.err.println("usage: StateDump <version> <out.json>");
            System.exit(2);
        }
        String version = args[0];
        Path out = Path.of(args[1]);

        Class.forName("net.minecraft.SharedConstants").getMethod("tryDetectVersion").invoke(null);
        Class.forName("net.minecraft.server.Bootstrap").getMethod("bootStrap").invoke(null);

        Field registryField = Class.forName("net.minecraft.world.level.block.Block")
                .getField("BLOCK_STATE_REGISTRY");
        Iterable<?> registry = (Iterable<?>) registryField.get(null);
        Object[] directions = Class.forName("net.minecraft.core.Direction").getEnumConstants();
        if (directions.length != 6) {
            throw new IllegalStateException("expected 6 directions, got " + directions.length);
        }

        List<Integer> emission = new ArrayList<>();
        List<Integer> dampening = new ArrayList<>();
        List<Integer> propagates = new ArrayList<>();
        List<Integer> canOcclude = new ArrayList<>();
        List<Integer> useShape = new ArrayList<>();
        List<Integer> hasCollision = new ArrayList<>();
        List<Integer> mapColorRgb = new ArrayList<>();
        List<Integer> fallingDustRgb = new ArrayList<>();
        List<Integer> isFallingBlock = new ArrayList<>();
        List<Integer> spawnTerrainParticles = new ArrayList<>();
        List<Integer> renderShapeInvisible = new ArrayList<>();
        List<Integer> solidRender = new ArrayList<>();
        List<Integer> blocksMotion = new ArrayList<>();
        List<Integer> leavesBlock = new ArrayList<>();
        List<Integer> liquidBlockContainer = new ArrayList<>();
        List<Integer> canPlaceWater = new ArrayList<>();
        List<Integer> bonemealType = new ArrayList<>();
        List<int[]> bonemealParticleOffset = new ArrayList<>();
        List<Integer> collisionShapeFullBlock = new ArrayList<>();
        List<Integer> canBeReplacedByWater = new ArrayList<>();
        List<Integer> dynamicShape = new ArrayList<>();
        List<Integer> shapeMaxYThirtySeconds = new ArrayList<>();
        Class<?> leavesBlockClass = Class.forName("net.minecraft.world.level.block.LeavesBlock");
        Class<?> liquidBlockContainerClass = Class.forName("net.minecraft.world.level.block.LiquidBlockContainer");
        Class<?> bonemealableClass = Class.forName("net.minecraft.world.level.block.BonemealableBlock");
        // state id -> 6 face masks (64 hex chars each), only for canOcclude && useShape states
        Map<Integer, String[]> faceMasks = new LinkedHashMap<>();

        Methods m = null;
        int id = 0;
        for (Object state : registry) {
            if (m == null) {
                m = new Methods(state.getClass());
            }
            boolean occludes = (Boolean) m.canOcclude.invoke(state);
            boolean shaped = (Boolean) m.useShapeForLightOcclusion.invoke(state);
            emission.add((Integer) m.getLightEmission.invoke(state));
            dampening.add((Integer) m.invokeWorld(m.getLightDampening, state));
            propagates.add(((Boolean) m.invokeWorld(m.propagatesSkylightDown, state)) ? 1 : 0);
            canOcclude.add(occludes ? 1 : 0);
            useShape.add(shaped ? 1 : 0);
            Object block = m.getBlock.invoke(state);
            hasCollision.add(m.hasCollision.getBoolean(block) ? 1 : 0);
            mapColorRgb.add(m.mapColor(state));
            boolean falling = m.fallingBlockClass.isInstance(block);
            isFallingBlock.add(falling ? 1 : 0);
            fallingDustRgb.add(falling ? m.fallingDustRgb(block, state) : 0);
            spawnTerrainParticles.add((Boolean) m.shouldSpawnTerrainParticles.invoke(state) ? 1 : 0);
            renderShapeInvisible.add("INVISIBLE".equals(m.getRenderShape.invoke(state).toString()) ? 1 : 0);
            solidRender.add((Boolean) m.isSolidRender.invoke(state) ? 1 : 0);
            blocksMotion.add((Boolean) m.blocksMotion.invoke(state) ? 1 : 0);
            leavesBlock.add(leavesBlockClass.isInstance(block) ? 1 : 0);
            boolean isLiquidContainer = liquidBlockContainerClass.isInstance(block);
            liquidBlockContainer.add(isLiquidContainer ? 1 : 0);
            canPlaceWater.add(isLiquidContainer && (Boolean) m.canPlaceLiquid.invoke(
                    block, null, m.emptyBlockGetter, m.zeroBlockPos, state, m.waterFluid
            ) ? 1 : 0);
            collisionShapeFullBlock.add((Boolean) m.isCollisionShapeFullBlock.invoke(state, m.emptyBlockGetter, m.zeroBlockPos) ? 1 : 0);
            canBeReplacedByWater.add((Boolean) m.canBeReplaced.invoke(state, m.waterFluid) ? 1 : 0);
            dynamicShape.add(m.dynamicShape.getBoolean(block) ? 1 : 0);
            double maxY = ((Number) m.shapeMax.invoke(m.getShape.invoke(state, m.emptyBlockGetter, m.zeroBlockPos), m.yAxis)).doubleValue();
            if (maxY == Double.NEGATIVE_INFINITY) {
                // Minecraft's empty VoxelShape.max(axis) sentinel.
                shapeMaxYThirtySeconds.add(255);
            } else {
                long maxY32 = Math.round(maxY * 32.0);
                if (Math.abs(maxY * 32.0 - maxY32) > 1.0e-5 || maxY32 < 0 || maxY32 > 64) {
                    throw new IllegalStateException("outline maxY is not representable in thirty-seconds at state " + id + ": " + maxY);
                }
                shapeMaxYThirtySeconds.add((int) maxY32);
            }
            if (bonemealableClass.isInstance(block)) {
                Object type = m.getBonemealType.invoke(block);
                String typeName = ((Enum<?>) type).name();
                int typeCode = switch (typeName) {
                    case "NEIGHBOR_SPREADER" -> 1;
                    case "GROWER" -> 2;
                    default -> throw new IllegalStateException("unknown BonemealableBlock.Type: " + typeName);
                };
                Object particlePos = m.getBonemealParticlePos.invoke(block, m.zeroBlockPos);
                bonemealType.add(typeCode);
                bonemealParticleOffset.add(new int[] {
                    ((Number) m.blockPosX.invoke(particlePos)).intValue(),
                    ((Number) m.blockPosY.invoke(particlePos)).intValue(),
                    ((Number) m.blockPosZ.invoke(particlePos)).intValue()
                });
            } else {
                bonemealType.add(0);
                bonemealParticleOffset.add(new int[] {0, 0, 0});
            }
            if (occludes && shaped) {
                String[] masks = new String[6];
                for (int d = 0; d < 6; d++) {
                    Object shape = m.invokeWorld(m.getFaceOcclusionShape, state, directions[d]);
                    masks[d] = maskHex(projectFace(shape, m, AXIS_BY_ORDINAL[d], id, d));
                }
                faceMasks.put(id, masks);
            }
            id++;
        }

        try (BufferedWriter w = Files.newBufferedWriter(out)) {
            w.write("{\n");
            w.write("  \"version\": \"" + version + "\",\n");
            w.write("  \"state_count\": " + id + ",\n");
            writeIntArray(w, "emission", emission);
            writeIntArray(w, "dampening", dampening);
            writeIntArray(w, "propagates_skylight_down", propagates);
            writeIntArray(w, "can_occlude", canOcclude);
            writeIntArray(w, "use_shape_for_light_occlusion", useShape);
            writeIntArray(w, "has_collision", hasCollision);
            writeIntArray(w, "map_color_rgb", mapColorRgb);
            writeIntArray(w, "falling_dust_rgb", fallingDustRgb);
            writeIntArray(w, "is_falling_block", isFallingBlock);
            writeIntArray(w, "spawn_terrain_particles", spawnTerrainParticles);
            writeIntArray(w, "render_shape_invisible", renderShapeInvisible);
            writeIntArray(w, "solid_render", solidRender);
            writeIntArray(w, "blocks_motion", blocksMotion);
            writeIntArray(w, "leaves_block", leavesBlock);
            writeIntArray(w, "liquid_block_container", liquidBlockContainer);
            writeIntArray(w, "can_place_water", canPlaceWater);
            writeIntArray(w, "bonemeal_type", bonemealType);
            writeVec3Array(w, "bonemeal_particle_offset", bonemealParticleOffset);
            writeIntArray(w, "collision_shape_full_block", collisionShapeFullBlock);
            writeIntArray(w, "can_be_replaced_by_water", canBeReplacedByWater);
            writeIntArray(w, "dynamic_shape", dynamicShape);
            writeIntArray(w, "shape_max_y_32nds", shapeMaxYThirtySeconds);
            w.write("  \"face_masks\": {");
            boolean first = true;
            for (Map.Entry<Integer, String[]> e : faceMasks.entrySet()) {
                if (!first) {
                    w.write(",");
                }
                first = false;
                w.write("\n    \"" + e.getKey() + "\": [");
                for (int d = 0; d < 6; d++) {
                    if (d > 0) {
                        w.write(", ");
                    }
                    w.write("\"" + e.getValue()[d] + "\"");
                }
                w.write("]");
            }
            w.write("\n  }\n}\n");
        }
        System.out.println("wrote " + id + " states (" + faceMasks.size()
                + " with face-occlusion shapes) to " + out);
    }

    private static void writeIntArray(BufferedWriter w, String key, List<Integer> values)
            throws IOException {
        w.write("  \"" + key + "\": [");
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < values.size(); i++) {
            if (i > 0) {
                sb.append(',');
            }
            sb.append(values.get(i));
        }
        w.write(sb.toString());
        w.write("],\n");
    }

    private static void writeVec3Array(BufferedWriter w, String key, List<int[]> values)
            throws IOException {
        w.write("  \"" + key + "\": [");
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < values.size(); i++) {
            if (i > 0) sb.append(',');
            int[] v = values.get(i);
            sb.append('[').append(v[0]).append(',').append(v[1]).append(',').append(v[2]).append(']');
        }
        w.write(sb.toString());
        w.write("],\n");
    }

    /** Projects a face shape's boxes onto the face plane as a 16x16 bit grid. */
    private static int[] projectFace(Object shape, Methods m, char axis, int stateId, int dir)
            throws Exception {
        int[] rows = new int[16]; // rows[v] bits over u
        if ((Boolean) m.shapeIsEmpty.invoke(shape)) {
            return rows;
        }
        List<?> boxes = (List<?>) m.toAabbs.invoke(shape);
        for (Object box : boxes) {
            double minX = m.aabb("minX").getDouble(box);
            double minY = m.aabb("minY").getDouble(box);
            double minZ = m.aabb("minZ").getDouble(box);
            double maxX = m.aabb("maxX").getDouble(box);
            double maxY = m.aabb("maxY").getDouble(box);
            double maxZ = m.aabb("maxZ").getDouble(box);
            double u0;
            double u1;
            double v0;
            double v1;
            switch (axis) {
                case 'Y' -> { u0 = minX; u1 = maxX; v0 = minZ; v1 = maxZ; }
                case 'Z' -> { u0 = minX; u1 = maxX; v0 = minY; v1 = maxY; }
                default -> { u0 = minZ; u1 = maxZ; v0 = minY; v1 = maxY; }
            }
            int iu0 = toSixteenth(u0, stateId, dir);
            int iu1 = toSixteenth(u1, stateId, dir);
            int iv0 = toSixteenth(v0, stateId, dir);
            int iv1 = toSixteenth(v1, stateId, dir);
            for (int v = iv0; v < iv1; v++) {
                for (int u = iu0; u < iu1; u++) {
                    rows[v] |= 1 << u;
                }
            }
        }
        return rows;
    }

    private static int toSixteenth(double coord, int stateId, int dir) {
        double scaled = coord * 16.0;
        long rounded = Math.round(scaled);
        if (Math.abs(scaled - rounded) > 1e-5) {
            throw new IllegalStateException("occlusion shape not 1/16-aligned: coord " + coord
                    + " at state " + stateId + " dir " + dir);
        }
        return (int) Math.max(0, Math.min(16, rounded));
    }

    private static String maskHex(int[] rows) {
        StringBuilder sb = new StringBuilder(64);
        for (int v = 0; v < 16; v++) {
            sb.append(String.format("%04x", rows[v] & 0xFFFF));
        }
        return sb.toString();
    }

    /** Resolved reflection handles; falls back across the 26.x / 1.21.x renames. */
    private static final class Methods {
        final Method getLightEmission;
        final Method getLightDampening;
        final Method getMapColor;
        final Method shouldSpawnTerrainParticles;
        final Method getRenderShape;
        final Method isSolidRender;
        final Method blocksMotion;
        final Method isCollisionShapeFullBlock;
        final Method canBeReplaced;
        final Method canPlaceLiquid;
        final Object waterFluid;
        final Method getShape;
        final Method shapeMax;
        final Object yAxis;
        final Field dynamicShape;
        final Method getBonemealType;
        final Method getBonemealParticlePos;
        final Method blockPosX;
        final Method blockPosY;
        final Method blockPosZ;
        final Class<?> fallingBlockClass;
        final Field mapColorCol;
        final Class<?> blockGetterClass;
        final Object emptyBlockGetter;
        final Object zeroBlockPos;
        final Method propagatesSkylightDown;
        final Method canOcclude;
        final Method useShapeForLightOcclusion;
        final Method getFaceOcclusionShape;
        final Method shapeIsEmpty;
        final Method toAabbs;
        final Method getBlock;
        final Field hasCollision;
        private final Class<?> aabbClass;
        private final Map<String, Field> aabbFields = new LinkedHashMap<>();

        final Object[] worldArgs;

        Methods(Class<?> stateClass) throws Exception {
            Class<?> direction = Class.forName("net.minecraft.core.Direction");
            getLightEmission = stateClass.getMethod("getLightEmission");
            canOcclude = stateClass.getMethod("canOcclude");
            useShapeForLightOcclusion = stateClass.getMethod("useShapeForLightOcclusion");

            Method dampening;
            Object[] wa;
            Class<?>[] worldTypes;
            try {
                dampening = firstMethod(stateClass, "getLightDampening", "getLightBlock");
                wa = new Object[0];
                worldTypes = new Class<?>[0];
            } catch (NoSuchMethodException e) {
                // Pre-1.21.2: the light/shape getters take a world context,
                // which vanilla's own state cache fed with the empty getter.
                Class<?> getter = Class.forName("net.minecraft.world.level.BlockGetter");
                Class<?> pos = Class.forName("net.minecraft.core.BlockPos");
                wa = new Object[] {
                    Class.forName("net.minecraft.world.level.EmptyBlockGetter")
                            .getEnumConstants()[0],
                    pos.getField("ZERO").get(null),
                };
                worldTypes = new Class<?>[] { getter, pos };
                dampening = stateClass.getMethod("getLightBlock", getter, pos);
            }
            worldArgs = wa;
            getLightDampening = dampening;
            propagatesSkylightDown = stateClass.getMethod("propagatesSkylightDown", worldTypes);
            Class<?>[] faceTypes = new Class<?>[worldTypes.length + 1];
            System.arraycopy(worldTypes, 0, faceTypes, 0, worldTypes.length);
            faceTypes[worldTypes.length] = direction;
            getFaceOcclusionShape = stateClass.getMethod("getFaceOcclusionShape", faceTypes);
            Class<?> voxelShape = Class.forName("net.minecraft.world.phys.shapes.VoxelShape");
            shapeIsEmpty = voxelShape.getMethod("isEmpty");
            toAabbs = voxelShape.getMethod("toAabbs");
            getBlock = stateClass.getMethod("getBlock");
            blockGetterClass = Class.forName("net.minecraft.world.level.BlockGetter");
            Class<?> blockPos = Class.forName("net.minecraft.core.BlockPos");
            getMapColor = stateClass.getMethod("getMapColor", blockGetterClass, blockPos);
            shouldSpawnTerrainParticles = stateClass.getMethod("shouldSpawnTerrainParticles");
            getRenderShape = stateClass.getMethod("getRenderShape");
            isSolidRender = stateClass.getMethod("isSolidRender");
            blocksMotion = stateClass.getMethod("blocksMotion");
            isCollisionShapeFullBlock = stateClass.getMethod("isCollisionShapeFullBlock", blockGetterClass, blockPos);
            Class<?> fluidClass = Class.forName("net.minecraft.world.level.material.Fluid");
            canBeReplaced = stateClass.getMethod("canBeReplaced", fluidClass);
            Class<?> liquidBlockContainer = Class.forName("net.minecraft.world.level.block.LiquidBlockContainer");
            canPlaceLiquid = liquidBlockContainer.getMethod(
                    "canPlaceLiquid",
                    Class.forName("net.minecraft.world.entity.LivingEntity"),
                    blockGetterClass,
                    blockPos,
                    stateClass,
                    fluidClass
            );
            waterFluid = Class.forName("net.minecraft.world.level.material.Fluids").getField("WATER").get(null);
            getShape = stateClass.getMethod("getShape", blockGetterClass, blockPos);
            Class<?> directionAxis = Class.forName("net.minecraft.core.Direction$Axis");
            yAxis = directionAxis.getEnumConstants()[1];
            shapeMax = voxelShape.getMethod("max", directionAxis);
            dynamicShape = Class.forName("net.minecraft.world.level.block.state.BlockBehaviour")
                    .getDeclaredField("dynamicShape");
            dynamicShape.setAccessible(true);
            Class<?> bonemealableClass = Class.forName("net.minecraft.world.level.block.BonemealableBlock");
            getBonemealType = bonemealableClass.getMethod("getType");
            getBonemealParticlePos = bonemealableClass.getMethod("getParticlePos", blockPos);
            blockPosX = blockPos.getMethod("getX");
            blockPosY = blockPos.getMethod("getY");
            blockPosZ = blockPos.getMethod("getZ");
            emptyBlockGetter = Class.forName("net.minecraft.world.level.EmptyBlockGetter")
                    .getField("INSTANCE").get(null);
            zeroBlockPos = blockPos.getField("ZERO").get(null);
            mapColorCol = Class.forName("net.minecraft.world.level.material.MapColor").getField("col");
            fallingBlockClass = Class.forName("net.minecraft.world.level.block.FallingBlock");
            hasCollision = Class.forName("net.minecraft.world.level.block.state.BlockBehaviour")
                    .getDeclaredField("hasCollision");
            hasCollision.setAccessible(true);
            aabbClass = Class.forName("net.minecraft.world.phys.AABB");
        }

        Field aabb(String name) throws Exception {
            Field f = aabbFields.get(name);
            if (f == null) {
                f = aabbClass.getField(name);
                aabbFields.put(name, f);
            }
            return f;
        }

        int mapColor(Object state) throws Exception {
            Object color = getMapColor.invoke(state, emptyBlockGetter, zeroBlockPos);
            return mapColorCol.getInt(color) & 0x00ff_ffff;
        }

        int fallingDustRgb(Object block, Object state) throws Exception {
            Method method = fallingBlockClass.getMethod("getDustColor", state.getClass(),
                    blockGetterClass, zeroBlockPos.getClass());
            return ((Number) method.invoke(block, state, emptyBlockGetter, zeroBlockPos)).intValue() & 0x00ff_ffff;
        }

        Object invokeWorld(Method method, Object state, Object... extra) throws Exception {
            Object[] args = new Object[worldArgs.length + extra.length];
            System.arraycopy(worldArgs, 0, args, 0, worldArgs.length);
            System.arraycopy(extra, 0, args, worldArgs.length, extra.length);
            return method.invoke(state, args);
        }

        private static Method firstMethod(Class<?> cls, String... names) throws NoSuchMethodException {
            for (String name : names) {
                try {
                    return cls.getMethod(name);
                } catch (NoSuchMethodException ignored) {
                    // try the next name
                }
            }
            throw new NoSuchMethodException(String.join("/", names));
        }
    }

    private StateDump() {}
}
