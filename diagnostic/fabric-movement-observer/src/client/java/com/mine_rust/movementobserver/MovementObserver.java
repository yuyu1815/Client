package com.mine_rust.movementobserver;

import com.google.gson.JsonObject;
import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientLifecycleEvents;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.fabricmc.fabric.api.client.networking.v1.ClientPlayConnectionEvents;
import net.fabricmc.fabric.api.client.keymapping.v1.KeyMappingHelper;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Minecraft;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.resources.Identifier;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.protocol.Packet;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.phys.Vec3;
import org.lwjgl.glfw.GLFW;
import io.netty.buffer.ByteBuf;
import io.netty.channel.Channel;
import io.netty.util.AttributeKey;

import java.io.BufferedWriter;
import java.util.Base64;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.security.MessageDigest;
import java.time.Instant;
import java.util.HexFormat;
import java.util.List;
import java.util.UUID;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicLong;

public final class MovementObserver implements ClientModInitializer {
    private static final int QUEUE = 128, MAX_ROW = 12 * 1024 * 1024;
    private static final long LIMIT = 1024L * 1024 * 1024;
    private static final int MAX_RAW = 8 * 1024 * 1024;
    private static final AttributeKey<Long> CONNECTION_EPOCH = AttributeKey.valueOf("movementobserver:connection_epoch");
        private static final AtomicLong CONNECTION_IDS = new AtomicLong();
    private static final ThreadLocal<RawStart> RAW = new ThreadLocal<>();
    private record RawStart(Channel channel, String direction, String phase, String packetType, int start) {}
    private static final AtomicBoolean ACTIVE = new AtomicBoolean();
    private static final AtomicBoolean WRITER_RUNNING = new AtomicBoolean();
    private static final ArrayBlockingQueue<String> ROWS = new ArrayBlockingQueue<>(QUEUE);
    private static final AtomicLong SEQ = new AtomicLong(), DROPPED = new AtomicLong(), RAW_OMITTED_BYTES = new AtomicLong();
    private static final ExecutorService WRITER = Executors.newSingleThreadExecutor(r -> { Thread t = new Thread(r, "movement-observer-writer"); t.setDaemon(true); return t; });
    private static final KeyMapping TOGGLE = KeyMappingHelper.registerKeyMapping(new KeyMapping("key.movementobserver.toggle", GLFW.GLFW_KEY_F8, KeyMapping.Category.register(Identifier.fromNamespaceAndPath("movementobserver", "diagnostic"))));
    private static volatile Path path;
    private static volatile long startedNanos, startWall;
    private static volatile String stopReason = "stop";
    private static volatile Minecraft client;

    @Override public void onInitializeClient() {
        client = Minecraft.getInstance();
        if (Boolean.getBoolean("movementobserver.classloadSmoke")) {
            for (String name : List.of("net.minecraft.client.multiplayer.ClientPacketListener", "net.minecraft.world.entity.Entity", "net.minecraft.world.entity.LivingEntity", "net.minecraft.world.entity.player.Player", "net.minecraft.client.player.LocalPlayer", "net.minecraft.network.Connection", "net.minecraft.network.PacketEncoder", "net.minecraft.network.PacketDecoder", "net.minecraft.client.KeyboardHandler", "net.minecraft.client.MouseHandler", "net.minecraft.client.Minecraft")) {
                try { Class.forName(name, false, Minecraft.class.getClassLoader()); System.out.println("[movementobserver] classloadSmoke PASS " + name); }
                catch (ClassNotFoundException e) { throw new IllegalStateException("classloadSmoke FAILED " + name, e); }
            }
        }
        ClientTickEvents.END_CLIENT_TICK.register(mc -> { while (TOGGLE.consumeClick()) { if (ACTIVE.get()) stop("user_stop"); else start(mc); } });
        ClientPlayConnectionEvents.DISCONNECT.register((handler, mc) -> stop("disconnect"));
        ClientLifecycleEvents.CLIENT_STOPPING.register(mc -> stop("end_game"));
        if (Boolean.getBoolean("movementobserver.smokeAutoStop")) ClientLifecycleEvents.CLIENT_STARTED.register(mc -> mc.execute(mc::stop));
    }

    private static synchronized void start(Minecraft mc) {
        if (ACTIVE.get()) return;
        if (WRITER_RUNNING.get()) {
            mc.gui.hud.getChat().addClientSystemMessage(net.minecraft.network.chat.Component.literal("Movement observer is still finishing the previous file; try F8 again shortly."));
            return;
        }
        try {
            Path dir = net.fabricmc.loader.api.FabricLoader.getInstance().getGameDir().resolve("movement-observations");
            Files.createDirectories(dir);
            path = dir.resolve("movement-" + UUID.randomUUID() + ".jsonl");
            startedNanos = System.nanoTime(); startWall = System.currentTimeMillis(); stopReason = "stop";
            SEQ.set(0); DROPPED.set(0); RAW_OMITTED_BYTES.set(0); ROWS.clear(); WRITER_RUNNING.set(true); ACTIVE.set(true);
            WRITER.execute(() -> { try { writeFile(path, mc); } finally { WRITER_RUNNING.set(false); } });
            mc.gui.hud.getChat().addClientSystemMessage(net.minecraft.network.chat.Component.literal("Movement observer recording: " + path));
        } catch (IOException e) { mc.gui.hud.getChat().addClientSystemMessage(net.minecraft.network.chat.Component.literal("Movement observer start failed: " + e)); }
    }
    private static synchronized void stop(String reason) { if (ACTIVE.compareAndSet(true, false)) stopReason = reason; }

    public static void frameStart() { if (ACTIVE.get()) { frameId++; frameStartNanos=System.nanoTime(); framePlayerTicks=0; } }
    public static void frameEnd(Minecraft mc) { if (!ACTIVE.get()) return; long now=System.nanoTime(); JsonObject d=new JsonObject(); d.addProperty("event","frame_observation"); d.addProperty("frame_id",frameId); d.addProperty("frame_nanos",Math.max(0L,now-frameStartNanos)); d.addProperty("frame_elapsed_sec",Math.max(0L,now-frameStartNanos)/1_000_000_000.0); d.addProperty("frame_player_tick_count",framePlayerTicks); try { d.addProperty("native_partial_ticks_f32",(double)mc.getDeltaTracker().getGameTimeDeltaPartialTick(false)); } catch(RuntimeException ignored) { d.add("native_partial_ticks_f32",null); } d.add("rust_accumulator",null); d.addProperty("rust_accumulator_reason","Minecraft DeltaTracker exposes partial ticks but not Rust simulation accumulator state"); offer("local","frame_observation",d); }
    public static boolean isRecordingFast() { return ACTIVE.get(); }
    public static boolean capturePhase(String phase) { return phase.equals("PLAY") || phase.equals("CONFIGURATION"); }
    public static void beginRaw(Channel channel, String direction, String phase, String packetType, int start) {
        RAW.remove();
        if (ACTIVE.get() && capturePhase(phase)) RAW.set(new RawStart(channel, direction, phase.toLowerCase(java.util.Locale.ROOT), packetType, start));
    }
    public static void endRaw(Channel channel, ByteBuf buffer, int end) { endRaw(channel, buffer, end, null); }
    public static void endRaw(Channel channel, ByteBuf buffer, int end, String packetType) {
        RawStart s = RAW.get(); RAW.remove();
        if (s == null || s.channel() != channel || !ACTIVE.get()) return;
        int length = end - s.start();
        if (length <= 0 || length > MAX_RAW || s.start() < 0 || end > buffer.writerIndex()) { DROPPED.incrementAndGet(); RAW_OMITTED_BYTES.addAndGet(Math.max(0, length)); return; }
        try {
            byte[] bytes = new byte[length]; buffer.getBytes(s.start(), bytes);
            int id = 0, shift = 0, idLength = 0; boolean validId = false;
            for (int i = 0; i < Math.min(length, 5); i++) { int b = bytes[i] & 255; if (i == 4 && (b & 0xf0) != 0) break; id |= (b & 127) << shift; shift += 7; if ((b & 128) == 0) { idLength = i + 1; validId = true; break; } }
            if (!validId || idLength == 0) id = -1;
            Long epoch = channel.attr(CONNECTION_EPOCH).get();
            if (epoch == null) { long assigned = CONNECTION_IDS.incrementAndGet(); Long prev = channel.attr(CONNECTION_EPOCH).setIfAbsent(assigned); epoch = prev == null ? assigned : prev; }
            JsonObject d = new JsonObject(); d.addProperty("protocol_state", s.phase()); d.addProperty("wire_protocol", net.minecraft.SharedConstants.getProtocolVersion());
            String traceId = channel.attr(PacketTrace.KEY).get(); if (traceId == null) traceId = UUID.randomUUID().toString();
            d.addProperty("connection_epoch", epoch); d.addProperty("packet_trace_id", traceId);
            if (id >= 0) d.addProperty("native_id", id); else d.add("native_id", null);
            if (packetType != null || s.packetType() != null) d.addProperty("packet_type", packetType == null ? s.packetType() : packetType);
            d.add("frame_id", frameId == 0 ? com.google.gson.JsonNull.INSTANCE : new com.google.gson.JsonPrimitive(frameId));
            d.addProperty("payload_base64", Base64.getEncoder().encodeToString(bytes)); d.addProperty("raw_length", length);
            d.addProperty("capture_point", "wire_plaintext"); d.addProperty("payload_layout", "id_plus_payload");
            offer(s.direction(), "packet_raw", d);
            if (s.direction().equals("inbound")) channel.attr(PacketTrace.KEY).set(traceId);
        } catch (RuntimeException e) { DROPPED.incrementAndGet(); }
    }
    public static void input(String kind, long window, int action, int code, int scan, int modifiers, double x, double y) {
        if (!ACTIVE.get() || client == null || window != client.getWindow().handle()) return;
        JsonObject d = new JsonObject(); d.addProperty("event", "input_event"); d.addProperty("kind", kind);
        d.addProperty("action", action); d.addProperty("code", code); d.addProperty("code_name", inputCodeName(kind, code)); d.addProperty("scancode", scan); d.addProperty("modifiers", modifiers);
        if (kind.equals("cursor") || kind.equals("scroll")) { d.addProperty("x", x); d.addProperty("y", y); }
        d.addProperty("screen", client.gui.screen() == null ? "none" : client.gui.screen().getClass().getSimpleName());
        d.addProperty("frame_id", frameId); offer("local", "input_event", d);
    }
    private static String inputCodeName(String kind, int code) {
        if (kind.equals("button")) return switch (code) { case GLFW.GLFW_MOUSE_BUTTON_LEFT -> "mouse_left"; case GLFW.GLFW_MOUSE_BUTTON_RIGHT -> "mouse_right"; case GLFW.GLFW_MOUSE_BUTTON_MIDDLE -> "mouse_middle"; default -> "unknown"; };
        return switch (code) { case GLFW.GLFW_KEY_W -> "W"; case GLFW.GLFW_KEY_A -> "A"; case GLFW.GLFW_KEY_S -> "S"; case GLFW.GLFW_KEY_D -> "D"; case GLFW.GLFW_KEY_SPACE -> "SPACE"; case GLFW.GLFW_KEY_LEFT_SHIFT -> "LEFT_SHIFT"; case GLFW.GLFW_KEY_RIGHT_SHIFT -> "RIGHT_SHIFT"; case GLFW.GLFW_KEY_LEFT_CONTROL -> "LEFT_CONTROL"; case GLFW.GLFW_KEY_RIGHT_CONTROL -> "RIGHT_CONTROL"; case GLFW.GLFW_KEY_ESCAPE -> "ESCAPE"; case GLFW.GLFW_KEY_ENTER -> "ENTER"; case GLFW.GLFW_KEY_UP -> "UP"; case GLFW.GLFW_KEY_DOWN -> "DOWN"; case GLFW.GLFW_KEY_LEFT -> "LEFT"; case GLFW.GLFW_KEY_RIGHT -> "RIGHT"; default -> "unknown"; };
    }
    public static void packet(Packet<?> p, String direction, String stage, String errorClass, String traceId) {
        if (!isRecordingFast()) return;
        try {
            JsonObject data = PacketFields.capture(p);
            if (data == null) { data = new JsonObject(); data.addProperty("packet", "unknown_typed"); data.add("fields", new JsonObject()); }
            data.addProperty("packet_type", p.type().id().toString());
            if (traceId != null) data.addProperty("packet_trace_id", traceId);
            data.add("native_id", null); // Numeric IDs are taken only from the original raw codec bytes.
            data.addProperty("native_stage", direction.equals("outbound") ? "channel_write" : "channel_read_before_listener");
            if (stage.equals("transport_write_failure") && errorClass != null) data.addProperty("error_class", errorClass);
            offer(direction, stage, data);
        } catch (RuntimeException ignored) { /* Diagnostics must never block or alter a packet. */ }
    }
    public static JsonObject blockState(net.minecraft.core.BlockPos pos) {
        try {
            JsonObject o = new JsonObject(); JsonArray p = new JsonArray(); p.add(pos.getX()); p.add(pos.getY()); p.add(pos.getZ()); o.add("block", p);
            var level = client == null ? null : client.level;
            if (level == null) o.add("state", null);
            else o.addProperty("state", net.minecraft.world.level.block.Block.BLOCK_STATE_REGISTRY.getId(level.getBlockState(pos)));
            return o;
        } catch (RuntimeException ignored) { return null; }
    }
    public static List<JsonObject> sectionStates(net.minecraft.network.protocol.game.ClientboundSectionBlocksUpdatePacket packet) {
        try { java.util.ArrayList<JsonObject> out = new java.util.ArrayList<>(64); int[] n={0};
            packet.runUpdates((pos,state)->{ if(n[0]++<4096) out.add(blockState(pos)); }); return out;
        } catch (RuntimeException ignored) { return List.of(); }
    }
    public static void applied(Packet<?> packet, String stage, List<JsonObject> blocks) {
        if (!isRecordingFast() || client == null || !client.isSameThread()) return;
        try {
            JsonObject d=PacketFields.capture(packet); if(d==null)return;
            d.add("native_id", null); d.addProperty("native_stage", "client_packet_listener_"+stage);
            JsonObject state=new JsonObject(); LocalPlayer p=client.player;
            if(p!=null && (packet instanceof net.minecraft.network.protocol.game.ClientboundPlayerPositionPacket || packet instanceof net.minecraft.network.protocol.game.ClientboundPlayerRotationPacket || (packet instanceof net.minecraft.network.protocol.game.ClientboundSetEntityMotionPacket m && m.id()==p.getId()) || (packet instanceof net.minecraft.network.protocol.game.ClientboundTeleportEntityPacket t && t.id()==p.getId()) || (packet instanceof net.minecraft.network.protocol.game.ClientboundEntityPositionSyncPacket s && s.id()==p.getId()) || (packet instanceof net.minecraft.network.protocol.game.ClientboundUpdateAttributesPacket a && a.getEntityId()==p.getId()))) state.add("player",playerState(p, stage, packet instanceof net.minecraft.network.protocol.game.ClientboundPlayerPositionPacket || packet instanceof net.minecraft.network.protocol.game.ClientboundPlayerRotationPacket));
            else state.add("player",null);
            if (p!=null && packet instanceof net.minecraft.network.protocol.game.ClientboundUpdateAttributesPacket a && a.getEntityId()==p.getId()) state.add("attributes",attributeState(p,a));
            JsonArray bs=new JsonArray(); blocks.forEach(bs::add); state.add("blocks",bs);
            if(packet instanceof net.minecraft.network.protocol.game.ClientboundBlockChangedAckPacket) { state.add("prediction_state",null); state.addProperty("prediction_state_semantics","Minecraft exposes no local prediction queue snapshot at this hook"); }
            d.add("applied_state",state); offer("inbound",stage,d);
        } catch (RuntimeException ignored) { /* Passive observation cannot change Minecraft packet handling. */ }
    }
    private static JsonObject playerState(LocalPlayer p, String stage, boolean correction) {
        JsonObject o=new JsonObject(); o.add("position",vec(p.position())); o.add("velocity",vec(p.getDeltaMovement()));
        if (correction) { JsonObject correctionShapes=CollisionSnapshots.capture(p, p.getBoundingBox(), Vec3.ZERO); correctionShapes.addProperty("correction_stage", stage); o.add("shape_snapshot", correctionShapes); }
        o.addProperty("on_ground",p.onGround()); o.addProperty("horizontal_collision",p.horizontalCollision); o.addProperty("yaw",(double)p.getYRot()); o.addProperty("pitch",(double)p.getXRot());
        o.addProperty("sprinting",p.isSprinting()); o.addProperty("crouching",p.isCrouching()); o.addProperty("swimming",p.isSwimming()); o.addProperty("in_water",p.isInWater()); o.addProperty("in_lava",p.isInLava()); o.addProperty("pose",p.getPose().name()); o.addProperty("food_level",p.getFoodData().getFoodLevel());
        playerConditions(o,p);
        return o;
    }
    private static JsonArray attributeState(LocalPlayer player, net.minecraft.network.protocol.game.ClientboundUpdateAttributesPacket packet) {
        JsonArray out=new JsonArray();
        for (var snapshot:packet.getValues()) {
            JsonObject a=new JsonObject(); a.addProperty("attribute",snapshot.attribute().unwrapKey().map(k->k.identifier().toString()).orElse("unknown"));
            var instance=player.getAttribute(snapshot.attribute());
            if(instance==null){a.add("base",null);a.add("modifiers",null);a.add("effective",null);}
            else {a.addProperty("base",instance.getBaseValue());JsonArray modifiers=new JsonArray();for(var m:instance.getModifiers()){JsonObject x=new JsonObject();x.addProperty("id",m.id().toString());x.addProperty("amount",m.amount());x.addProperty("operation",m.operation().name().toLowerCase(java.util.Locale.ROOT));modifiers.add(x);}a.add("modifiers",modifiers);a.addProperty("effective",instance.getValue());}
            out.add(a);
        }
        return out;
    }
    private static final ThreadLocal<MoveCapture> MOVE = new ThreadLocal<>();
    private static final ThreadLocal<AirCapture> AIR = new ThreadLocal<>();
    private static final ThreadLocal<JumpCapture> JUMP = new ThreadLocal<>();
    private static final ThreadLocal<JsonObject> PHYSICS = new ThreadLocal<>();
    private static JsonObject TICK_TRAVEL = new JsonObject();
    private static volatile long frameId, frameStartNanos;
    private static int framePlayerTicks;
    private static final class JumpCapture {
        final JsonObject data = new JsonObject();
        boolean powerSeen, factorSeen;
    }
    private static final class AirCapture {
        final JsonObject data = new JsonObject();
        boolean onGround;
        boolean used;
        float friction;
    }
    private static final class MoveCapture {
        final JsonObject data = new JsonObject();
        Vec3 clipped;
    }
    public static void beginJump(net.minecraft.world.entity.LivingEntity entity) {
        if (!ACTIVE.get() || client == null || entity != client.player) return;
        try { JumpCapture c = new JumpCapture(); c.data.addProperty("event", "jump_observation"); c.data.addProperty("sprinting_before_jump", entity.isSprinting()); c.data.add("velocity_before", vec(entity.getDeltaMovement())); JUMP.set(c); }
        catch (RuntimeException ignored) { JUMP.remove(); }
    }
    public static void jumpPower(net.minecraft.world.entity.LivingEntity entity, float power) {
        JumpCapture c = JUMP.get(); if (c != null && client != null && entity == client.player) { c.powerSeen = true; c.data.addProperty("jump_power_f32", (double)power); c.data.addProperty("zero_power_early_return", power <= 1.0E-5f); }
    }
    public static void blockJumpFactor(net.minecraft.world.entity.LivingEntity entity, float factor) {
        JumpCapture c = JUMP.get(); if (c != null && client != null && entity == client.player) { c.factorSeen = true; c.data.addProperty("used_block_jump_factor_f32", (double)factor); }
    }
    public static void endJump(net.minecraft.world.entity.LivingEntity entity) {
        JumpCapture c = JUMP.get(); JUMP.remove(); if (c == null || client == null || entity != client.player) return;
        if (!c.powerSeen) c.data.add("jump_power_f32", null);
        if (!c.factorSeen) { c.data.add("used_block_jump_factor_f32", null); c.data.addProperty("block_jump_factor_reason", "call not observed in this jump branch"); }
        c.data.add("velocity_after", vec(entity.getDeltaMovement())); mergeTravel(c.data); offer("local", "jump_observation", c.data);
    }
    public static void beginPhysics(net.minecraft.world.entity.LivingEntity entity, String branch) {
        if (!ACTIVE.get() || client == null || entity != client.player) return;
        JsonObject o = new JsonObject(); o.addProperty("branch", branch); PHYSICS.set(o);
    }
    public static void physicsSpeed(net.minecraft.world.entity.LivingEntity entity, float speed) { JsonObject o=PHYSICS.get(); if(o!=null && client!=null && entity==client.player) o.addProperty("native_move_relative_speed_f32", (double)speed); }
    public static void physicsMoveRelative(net.minecraft.world.entity.LivingEntity entity, float speed, Vec3 input) {
        if (!ACTIVE.get() || client == null || entity != client.player) return;
        JsonObject o = PHYSICS.get();
        if (o != null) { o.addProperty("move_relative_speed_f32", (double)speed); o.add("move_relative_input_vec3", vec(input)); }
    }
    public static void creativeVerticalDrag(net.minecraft.world.entity.player.Player player, double drag) {
        if (ACTIVE.get() && client != null && player == client.player) TICK_TRAVEL.addProperty("creative_vertical_drag_f64", drag);
    }
    public static void physicsScale(net.minecraft.world.entity.LivingEntity entity, double scale) { physicsDrag(entity,scale,scale,scale); }
    public static void physicsDrag(net.minecraft.world.entity.LivingEntity entity, double x, double y, double z) { JsonObject o=PHYSICS.get(); if(o!=null && client!=null && entity==client.player) { JsonArray a=new JsonArray(); a.add(x); a.add(y); a.add(z); o.add("native_axis_drag_f64",a); } }
    public static void physicsGravity(net.minecraft.world.entity.LivingEntity entity, double gravity) { JsonObject o=PHYSICS.get(); if(o!=null && client!=null && entity==client.player) { o.addProperty("native_effective_gravity_f64",gravity); o.addProperty("actual_gravity_f64",gravity); } }
    public static void physicsAdjustedGravity(net.minecraft.world.entity.LivingEntity entity, double gravity) { JsonObject o=PHYSICS.get(); if(o!=null && client!=null && entity==client.player) o.addProperty("native_fluid_adjustment_gravity_f64",gravity); }
    public static void endPhysics(net.minecraft.world.entity.LivingEntity entity) { JsonObject o=PHYSICS.get(); PHYSICS.remove(); if(o!=null && client!=null && entity==client.player) { for(String key:List.of("native_move_relative_speed_f32","native_axis_drag_f64","native_effective_gravity_f64","actual_gravity_f64","native_fluid_adjustment_gravity_f64","move_relative_speed_f32","move_relative_input_vec3")) if(!o.has(key)) o.add(key,null); TICK_TRAVEL.add("native_physics",o.deepCopy()); TICK_TRAVEL.add("actual_gravity_f64",o.has("actual_gravity_f64")?o.get("actual_gravity_f64").deepCopy():com.google.gson.JsonNull.INSTANCE); if(o.has("actual_gravity_f64")&&!o.get("actual_gravity_f64").isJsonNull()) TICK_TRAVEL.remove("actual_gravity_reason"); } }
    public static void beginAirTravel(net.minecraft.world.entity.LivingEntity entity) {
        if (!ACTIVE.get() || client == null || entity != client.player) return;
        try { beginPhysics(entity, "air"); AirCapture c = new AirCapture(); c.onGround = entity.onGround(); c.data.addProperty("event", "travel_observation"); c.data.addProperty("branch", "air"); c.data.addProperty("on_ground_at_start", c.onGround); c.data.addProperty("on_ground_at_land_start", c.onGround); AIR.set(c); }
        catch (RuntimeException ignored) { AIR.remove(); }
    }
    public static void airFrictionSource(net.minecraft.world.entity.LivingEntity entity, net.minecraft.core.BlockPos pos) {
        AirCapture c = AIR.get(); if (c != null && client != null && entity == client.player) c.data.add("friction_source_pos", blockPos(pos));
    }
    public static void airBlockFriction(net.minecraft.world.entity.LivingEntity entity, float friction) {
        AirCapture c = AIR.get(); if (c != null && client != null && entity == client.player) { c.used = true; c.friction = friction; c.data.addProperty("used_friction_f32", (double)friction); }
    }
    public static void airGroundDrag(net.minecraft.world.entity.LivingEntity entity, float airDrag) {
        AirCapture c = AIR.get();
        if (c != null && client != null && entity == client.player && c.used) {
            c.data.addProperty("used_ground_drag_f32", (double)(c.friction * airDrag));
        }
    }
    public static void endAirTravel(net.minecraft.world.entity.LivingEntity entity) {
        AirCapture c = AIR.get(); AIR.remove(); if (c == null || client == null || entity != client.player) return;
        if (!c.used) { c.data.add("used_friction_f32", null); c.data.addProperty("used_friction_reason", "friction helper invocation was not observed"); }
        if (!c.onGround) { c.data.add("friction_source_pos", null); c.data.addProperty("friction_source_reason", "airborne branch uses 1.0f; no block friction source is used"); }
        endPhysics(entity); mergeTravel(c.data); offer("local", "travel_observation", c.data);
    }
    private static JsonArray blockPos(net.minecraft.core.BlockPos p) {
        if (p == null) return null;
        JsonArray a = new JsonArray(); a.add(p.getX()); a.add(p.getY()); a.add(p.getZ()); return a;
    }
    public static void beginMove(Entity entity, net.minecraft.world.entity.MoverType type, Vec3 requested) {
        if (!ACTIVE.get() || client == null || entity != client.player) return;
        try {
            MoveCapture c = new MoveCapture(); MOVE.set(c);
            c.data.addProperty("event", "collision_move"); c.data.addProperty("mover_type", type.name());
            c.data.add("move_requested_delta", vec(requested)); c.data.add("bbox_before", box(entity.getBoundingBox()));
            c.data.addProperty("pose_at_move", entity.getPose().name());
            c.data.add("support_before", blockPos(entity.mainSupportingBlockPos.orElse(null)));
            c.data.addProperty("on_ground_before", entity.onGround());
        } catch (RuntimeException ignored) { MOVE.remove(); }
    }
    public static void usedStepHeight(Entity entity, float height) {
        MoveCapture c = MOVE.get(); if (c != null && client != null && entity == client.player) c.data.addProperty("used_step_height", (double)height);
    }
    public static void usedBlockSpeedFactor(Entity entity, float factor) {
        MoveCapture c = MOVE.get(); if (c != null && client != null && entity == client.player) c.data.addProperty("used_block_speed_factor_f32", (double)factor);
    }
    public static void collided(Entity entity, Vec3 requested, Vec3 clipped) {
        MoveCapture c = MOVE.get();
        if (c == null || client == null || entity != client.player) return;
        c.clipped = clipped; c.data.add("requested_delta", vec(requested)); c.data.add("clipped_delta", vec(clipped));
        if (!requested.equals(clipped)) c.data.add("shape_snapshot", CollisionSnapshots.capture((LocalPlayer)entity, entity.getBoundingBox(), requested));
        c.data.addProperty("original_requested_y_negative", requested.y < 0.0);
        c.data.addProperty("final_y_clipped", requested.y != clipped.y);
    }
    public static void endMove(Entity entity) {
        MoveCapture c = MOVE.get(); MOVE.remove();
        if (c == null || client == null || entity != client.player) return;
        try {
            if (c.clipped == null) { c.data.add("requested_delta", null); c.data.add("clipped_delta", null); c.data.addProperty("collision_result_reason", "Entity.collide was not invoked in this move branch"); }
            c.data.add("bbox_after", box(entity.getBoundingBox()));
            c.data.add("support_after", blockPos(entity.mainSupportingBlockPos.orElse(null)));
            c.data.addProperty("on_ground_after", entity.onGround());
            c.data.addProperty("horizontal_collision", entity.horizontalCollision);
            c.data.addProperty("vertical_collision", entity.verticalCollision);
            // The vanilla move result has now updated onGround; observe that result rather than infer it from a later snapshot.
            c.data.addProperty("ground_decision", entity.onGround());
            c.data.addProperty("vertical_collision_below", entity.verticalCollisionBelow);
            mergeTravel(c.data); offer("local", "collision_move", c.data);
        } catch (RuntimeException ignored) { /* read-only diagnostic */ }
    }
    private static void mergeTravel(JsonObject source) {
        if (source.has("friction_source_pos") || source.has("used_friction_f32")) {
            TICK_TRAVEL.add("friction_source_pos", source.has("friction_source_pos") ? source.get("friction_source_pos").deepCopy() : com.google.gson.JsonNull.INSTANCE);
            TICK_TRAVEL.add("used_friction_f32", source.has("used_friction_f32") ? source.get("used_friction_f32").deepCopy() : com.google.gson.JsonNull.INSTANCE);
            TICK_TRAVEL.add("used_ground_drag_f32", source.has("used_ground_drag_f32") ? source.get("used_ground_drag_f32").deepCopy() : com.google.gson.JsonNull.INSTANCE);
        }
        if (source.has("on_ground_at_land_start")) TICK_TRAVEL.add("on_ground_at_land_start", source.get("on_ground_at_land_start").deepCopy());
        if (source.has("jump_power_f32")) TICK_TRAVEL.add("jump_power_f32", source.get("jump_power_f32").deepCopy());
        for (String key : List.of("native_physics", "actual_gravity_f64")) if(source.has(key)) TICK_TRAVEL.add(key,source.get(key).deepCopy());
        if (source.has("used_block_jump_factor_f32")) TICK_TRAVEL.add("used_block_jump_factor_f32", source.get("used_block_jump_factor_f32").deepCopy());
        for (String key : List.of("used_step_height", "used_block_speed_factor_f32", "pose_at_move", "bbox_before", "bbox_after", "support_before", "support_after", "requested_delta", "clipped_delta", "original_requested_y_negative", "final_y_clipped", "ground_decision", "shape_snapshot")) {
            if (!source.has(key)) continue;
            JsonElement value = source.get(key);
            if (value != null && !value.isJsonNull()) {
                if (key.equals("bbox_before") || key.equals("bbox_after")) value = rustBox(value);
                else if (key.equals("requested_delta") || key.equals("clipped_delta")) value = rustVec(value);
            }
            TICK_TRAVEL.add(key, value.deepCopy());
        }
    }
    private static JsonArray rustVec(JsonElement value) {
        if (value == null || value.isJsonNull()) return null;
        JsonObject v = value.getAsJsonObject(); JsonArray a = new JsonArray(); a.add(v.get("x")); a.add(v.get("y")); a.add(v.get("z")); return a;
    }
    private static JsonArray rustBox(JsonElement value) {
        if (value == null || value.isJsonNull()) return null;
        JsonObject b = value.getAsJsonObject(); JsonArray a = new JsonArray(); a.add(rustVec(b.get("min"))); a.add(rustVec(b.get("max"))); return a;
    }
    private static void annotateNullReasons(JsonObject travel) {
        JsonObject reasons = new JsonObject();
        for (String key : TRAVEL_OBSERVATION_KEYS) {
            JsonElement value = travel.get(key);
            if (value == null || value.isJsonNull()) {
                String reason = switch (key) {
                    case "entity_shapes", "entity_shapes_truncated", "entity_shapes_omitted", "entity_shapes_max" -> "shape requery is nested in shape_snapshot only after collision clipping or a correction; it is not an actual resolver-input capture";
                    case "context" -> "collision resolver input context is unavailable; CollisionContext.of(player) is used only for the bounded diagnostic requery";
                    case "frame_nanos", "frame_elapsed_sec" -> "full runTick duration is emitted in frame_observation after LocalPlayer.tick, so it is not available in this tick snapshot";
                    case "frame_id", "frame_player_tick_count", "native_partial_ticks_f32" -> "runTick frame hook was not active for this LocalPlayer.tick; these samples are emitted in frame_observation when available";
                    case "actual_gravity_f64" -> "getEffectiveGravity was not invoked in this travel branch; values are captured only at hooked air, fluid, and fall-flying call sites";
                    case "native_physics" -> "no hooked air, water, lava, or fall-flying physics call site completed for this player tick";
                    case "ground_decision" -> "Entity.move result hook was not observed in this tick";
                    default -> "this actual vanilla call site was not observed in the current tick or branch";
                };
                reasons.addProperty(key, reason);
            }
        }
        travel.add("unavailable_reasons", reasons);
    }
    private static final List<String> TRAVEL_OBSERVATION_KEYS = List.of("friction_source_pos", "used_friction_f32", "used_ground_drag_f32", "on_ground_at_land_start", "jump_power_f32", "used_block_jump_factor_f32", "used_block_speed_factor_f32", "used_step_height", "pose_at_move", "bbox_before", "bbox_after", "support_before", "support_after", "requested_delta", "clipped_delta", "original_requested_y_negative", "final_y_clipped", "ground_decision", "entity_shapes", "entity_shapes_truncated", "entity_shapes_omitted", "entity_shapes_max", "context", "frame_nanos", "frame_elapsed_sec", "frame_id", "frame_player_tick_count", "native_partial_ticks_f32", "native_physics", "actual_gravity_f64", "creative_vertical_drag_f64");
    private static JsonObject emptyTravel() {
        JsonObject o = new JsonObject();
        for (String key : List.of("friction_source_pos", "used_friction_f32", "used_ground_drag_f32", "on_ground_at_land_start", "jump_power_f32", "used_block_jump_factor_f32", "used_block_speed_factor_f32", "used_step_height", "pose_at_move", "bbox_before", "bbox_after", "support_before", "support_after", "requested_delta", "clipped_delta", "original_requested_y_negative", "final_y_clipped", "ground_decision", "entity_shapes", "entity_shapes_truncated", "entity_shapes_omitted", "context", "frame_nanos", "frame_elapsed_sec", "frame_id", "frame_player_tick_count", "native_partial_ticks_f32", "native_physics", "actual_gravity_f64", "creative_vertical_drag_f64")) o.add(key, null);
        o.addProperty("entity_shapes_max", 8);
        o.addProperty("entity_shapes_semantics", "null: actual collision resolver collider inputs not captured by this observer");
        o.addProperty("ground_decision_semantics", "captured entity.onGround immediately after Entity.move returns; represents vanilla-updated result, not an independently inferred decision");
        o.addProperty("semantics", "values captured at actual vanilla hook call sites; null means not observed or unsupported, never snapshot-requeried");
        o.addProperty("available_fields", "used_friction_f32,friction_source_pos,used_ground_drag_f32,on_ground_at_land_start,jump_power_f32,used_block_jump_factor_f32,used_block_speed_factor_f32,used_step_height,pose_at_move,bbox_before,bbox_after,support_before,support_after,requested_delta,clipped_delta,original_requested_y_negative,final_y_clipped,ground_decision,actual_gravity_f64,native_physics,creative_vertical_drag_f64,frame_id,frame_player_tick_count,native_partial_ticks_f32");
        o.addProperty("unsupported_fields", "resolver/block/entity shapes, correction shape snapshot, LivingEntity.travelFlying alternate branch, true runTick duration at LocalPlayer.tick");
        o.add("creative_vertical_drag_f64",null); o.add("actual_gravity_f64",null); o.addProperty("actual_gravity_reason","effective gravity invocation not observed in this travel branch");
        o.add("native_physics",null); o.add("frame_nanos",null); o.add("frame_id",null); o.add("frame_elapsed_sec",null); o.add("frame_player_tick_count",null); o.add("native_partial_ticks_f32",null); o.addProperty("frame_accumulator_reason","Minecraft DeltaTracker exposes partial ticks but not Rust simulation accumulator state");
        return o;
    }
    public static void tick(LocalPlayer p, String stage) {
        if (!ACTIVE.get()) return;
        if (stage.equals("before_tick")) TICK_TRAVEL = emptyTravel();
        if (stage.equals("before_tick") && ACTIVE.get()) framePlayerTicks++;
        JsonObject d = new JsonObject();
        d.addProperty("event", "movement_tick");
        d.addProperty("tick", p.tickCount);
        d.add("position", vec(p.position())); d.add("velocity", vec(p.getDeltaMovement()));
        d.addProperty("on_ground", p.onGround()); d.addProperty("horizontal_collision", p.horizontalCollision);
        d.addProperty("yaw", p.getYRot()); d.addProperty("pitch", p.getXRot());
        d.addProperty("sprinting", p.isSprinting()); d.addProperty("crouching", p.isCrouching());
        d.addProperty("swimming", p.isSwimming()); d.addProperty("in_water", p.isInWater()); d.addProperty("in_lava", p.isInLava());
        d.addProperty("pose", p.getPose().name()); d.add("bbox", box(p.getBoundingBox()));
        d.addProperty("forward_input", p.input.getMoveVector().y); d.addProperty("sideways_input", p.input.getMoveVector().x);
        d.addProperty("jump_input", p.input.keyPresses.jump()); d.addProperty("shift_input", p.input.keyPresses.shift()); d.addProperty("sprint_input", p.input.keyPresses.sprint());
        d.addProperty("effective_movement_speed_f32", (float)p.getAttributeValue(Attributes.MOVEMENT_SPEED));
        d.addProperty("effective_jump_strength_f32", (float)p.getAttributeValue(Attributes.JUMP_STRENGTH));
        d.addProperty("effective_gravity", p.getAttributeValue(Attributes.GRAVITY));
        d.addProperty("step_height", p.maxUpStep());
        d.addProperty("food_level", p.getFoodData().getFoodLevel());
        d.addProperty("attributes_semantics", "effective values at snapshot; not a per-travel used-value capture");
        playerConditions(d,p);
        if (stage.equals("after_tick")) {
            if (ACTIVE.get() && frameStartNanos != 0L) { TICK_TRAVEL.addProperty("frame_id",frameId); TICK_TRAVEL.addProperty("frame_player_tick_count",framePlayerTicks); TICK_TRAVEL.addProperty("frame_nanos",(Number)null); TICK_TRAVEL.addProperty("frame_elapsed_sec",(Number)null); try { TICK_TRAVEL.addProperty("native_partial_ticks_f32",(double)client.getDeltaTracker().getGameTimeDeltaPartialTick(false)); } catch(RuntimeException ignored) { TICK_TRAVEL.add("native_partial_ticks_f32",null); } }
            JsonObject travel = TICK_TRAVEL.deepCopy();
            annotateNullReasons(travel);
            d.add("travel_observation", travel);
        }
        offer("local", stage, d);
    }
    private static void playerConditions(JsonObject o, LocalPlayer p) {
        var activeEffects=p.getActiveEffects().stream().sorted(java.util.Comparator.comparing(e -> {
            var key=BuiltInRegistries.MOB_EFFECT.getKey(e.getEffect().value()); return key==null?"unknown":key.toString();
        })).toList();
        JsonArray effects = new JsonArray();
        for (var effect : activeEffects.stream().limit(64).toList()) { JsonObject e=new JsonObject(); var key=BuiltInRegistries.MOB_EFFECT.getKey(effect.getEffect().value()); e.addProperty("id", key==null?"unknown":key.toString()); e.addProperty("amplifier",effect.getAmplifier()); e.addProperty("duration",effect.getDuration()); effects.add(e); }
        o.add("effects",effects); o.addProperty("effects_max",64); o.addProperty("effects_truncated",activeEffects.size()>64);
        var a=p.getAbilities(); JsonObject abilities=new JsonObject(); abilities.addProperty("flying",a.flying); abilities.addProperty("may_fly",a.mayfly); abilities.addProperty("fly_speed",a.getFlyingSpeed()); abilities.addProperty("walk_speed",a.getWalkingSpeed()); abilities.addProperty("invulnerable",a.invulnerable); abilities.addProperty("instabuild",a.instabuild); o.add("abilities",abilities);
    }
    private static JsonObject vec(Vec3 v) { JsonObject a = new JsonObject(); a.addProperty("x",v.x); a.addProperty("y",v.y); a.addProperty("z",v.z); return a; }
    private static JsonObject box(net.minecraft.world.phys.AABB b) { JsonObject a = new JsonObject(); a.add("min", vec(new Vec3(b.minX,b.minY,b.minZ))); a.add("max", vec(new Vec3(b.maxX,b.maxY,b.maxZ))); return a; }
    private static synchronized void offer(String direction, String stage, JsonObject data) {
        long seq = SEQ.incrementAndGet();
        JsonObject r = new JsonObject(); r.addProperty("seq",seq); r.addProperty("offset_us",(System.nanoTime()-startedNanos)/1000); r.addProperty("direction",direction); r.addProperty("stage",stage); r.add("data",data);
        String line = r.toString(); if (line.getBytes(StandardCharsets.UTF_8).length > MAX_ROW || !ROWS.offer(line)) { DROPPED.incrementAndGet(); if (stage.equals("packet_raw")) RAW_OMITTED_BYTES.addAndGet(data.get("raw_length").getAsLong()); }
    }
    private static void writeFile(Path file, Minecraft mc) {
        long bytes=0, written=0, oversize=0; String error=null;
        try (BufferedWriter out=Files.newBufferedWriter(file, StandardCharsets.UTF_8, StandardOpenOption.CREATE_NEW)) {
            JsonObject h=new JsonObject(); h.addProperty("type","header"); h.addProperty("schema",1); h.addProperty("utc_start_unix_ms",startWall); h.addProperty("wire_protocol",net.minecraft.SharedConstants.getProtocolVersion()); h.addProperty("client_kind","fabric"); h.add("executable", identity(ProcessHandle.current().info().command().orElse(null))); h.add("minecraft_artifact", null); h.addProperty("source_build_revision", buildRevision()); h.addProperty("queue_capacity",QUEUE); h.addProperty("size_limit_bytes",LIMIT); h.addProperty("max_row_bytes",MAX_ROW); h.addProperty("semantics","client observations only; queued != transport write completion != server acceptance");
            String header = h.toString(); out.write(header); out.newLine(); bytes = header.getBytes(StandardCharsets.UTF_8).length + 1;
            while (ACTIVE.get() || !ROWS.isEmpty()) {
                String row=ROWS.poll(100, java.util.concurrent.TimeUnit.MILLISECONDS);
                if (row != null) {
                    int size = row.getBytes(StandardCharsets.UTF_8).length + 1;
                    if (size > MAX_ROW) { oversize++; discardRow(row); }
                    else if (bytes + size > LIMIT - 4096) { stop("size_limit"); discardRow(row); }
                    else { out.write(row); out.newLine(); bytes += size; written++; if (bytes >= LIMIT) stop("size_limit"); }
                }
            }
            JsonObject f=new JsonObject(); f.addProperty("type","footer"); f.addProperty("utc_end_unix_ms",System.currentTimeMillis()); f.addProperty("offset_us",(System.nanoTime()-startedNanos)/1000); f.addProperty("last_seq",SEQ.get()); f.addProperty("written",written); f.addProperty("dropped",DROPPED.get()); f.addProperty("oversize_omitted",oversize); f.addProperty("raw_omitted_bytes",RAW_OMITTED_BYTES.get()); f.addProperty("reason",stopReason); f.addProperty("complete",DROPPED.get()==0 && oversize==0 && !stopReason.equals("size_limit")); out.write(f.toString()); out.newLine(); out.flush();
        } catch(Exception e){ACTIVE.set(false); error=e.toString(); e.printStackTrace();}
        if(mc!=null && mc.gui!=null) { String msg=error==null?"Movement observer saved: "+file+" (dropped="+DROPPED.get()+")":"Movement observer FAILED: "+error+"; "+file; mc.execute(()->mc.gui.hud.getChat().addClientSystemMessage(net.minecraft.network.chat.Component.literal(msg))); }
    }
    private static void discardRow(String row) {
        DROPPED.incrementAndGet();
        try {
            JsonObject event = com.google.gson.JsonParser.parseString(row).getAsJsonObject();
            if (event.get("stage").getAsString().equals("packet_raw")) RAW_OMITTED_BYTES.addAndGet(event.getAsJsonObject("data").get("raw_length").getAsLong());
        } catch (RuntimeException ignored) { /* row already counted as omitted */ }
    }
    private static String buildRevision() {
        try (var in = MovementObserver.class.getResourceAsStream("/observer.properties")) {
            if (in == null) return null;
            java.util.Properties p = new java.util.Properties(); p.load(in); return p.getProperty("sourceRevision");
        } catch (Exception e) { return null; }
    }
    private static JsonObject identity(String raw) {
        if(raw==null)return null;
        try { Path p=Path.of(raw); JsonObject o=new JsonObject(); o.addProperty("path",p.toString()); if(Files.isRegularFile(p)){o.addProperty("size_bytes",Files.size(p));o.addProperty("mtime_unix_ms",Files.getLastModifiedTime(p).toMillis()); MessageDigest md=MessageDigest.getInstance("SHA-256"); try(var in=Files.newInputStream(p)){byte[] b=new byte[65536];int n;while((n=in.read(b))>0)md.update(b,0,n);}o.addProperty("sha256",HexFormat.of().formatHex(md.digest()));} else {o.add("size_bytes",null);o.add("mtime_unix_ms",null);o.add("sha256",null);} o.addProperty("semantics","current Java process executable; not Minecraft artifact");return o;}catch(Exception e){return null;}
    }
    public MovementObserver() {}
}
