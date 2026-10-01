package com.mine_rust.movementobserver;

import com.google.gson.JsonObject;
import com.google.gson.JsonArray;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientLifecycleEvents;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.fabricmc.fabric.api.client.networking.v1.ClientPlayConnectionEvents;
import net.fabricmc.fabric.api.client.keymapping.v1.KeyMappingHelper;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Minecraft;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.resources.Identifier;
import net.minecraft.network.protocol.Packet;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.phys.Vec3;
import org.lwjgl.glfw.GLFW;

import java.io.BufferedWriter;
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
    private static final int QUEUE = 128, MAX_ROW = 512 * 1024;
    private static final long LIMIT = 64L * 1024 * 1024;
    private static final AtomicBoolean ACTIVE = new AtomicBoolean();
    private static final ArrayBlockingQueue<String> ROWS = new ArrayBlockingQueue<>(QUEUE);
    private static final AtomicLong SEQ = new AtomicLong(), DROPPED = new AtomicLong();
    private static final ExecutorService WRITER = Executors.newSingleThreadExecutor(r -> { Thread t = new Thread(r, "movement-observer-writer"); t.setDaemon(true); return t; });
    private static final KeyMapping TOGGLE = KeyMappingHelper.registerKeyMapping(new KeyMapping("key.movementobserver.toggle", GLFW.GLFW_KEY_F8, KeyMapping.Category.register(Identifier.fromNamespaceAndPath("movementobserver", "diagnostic"))));
    private static volatile Path path;
    private static volatile long startedNanos, startWall;
    private static volatile String stopReason = "stop";
    private static volatile Minecraft client;

    @Override public void onInitializeClient() {
        client = Minecraft.getInstance();
        ClientTickEvents.END_CLIENT_TICK.register(mc -> { while (TOGGLE.consumeClick()) { if (ACTIVE.get()) stop("user_stop"); else start(mc); } });
        ClientPlayConnectionEvents.DISCONNECT.register((handler, mc) -> stop("disconnect"));
        ClientLifecycleEvents.CLIENT_STOPPING.register(mc -> stop("end_game"));
    }

    private static synchronized void start(Minecraft mc) {
        if (ACTIVE.get()) return;
        try {
            Path dir = net.fabricmc.loader.api.FabricLoader.getInstance().getGameDir().resolve("movement-observations");
            Files.createDirectories(dir);
            path = dir.resolve("movement-" + UUID.randomUUID() + ".jsonl");
            startedNanos = System.nanoTime(); startWall = System.currentTimeMillis(); stopReason = "stop";
            SEQ.set(0); DROPPED.set(0); ROWS.clear(); ACTIVE.set(true);
            WRITER.execute(() -> writeFile(path, mc));
            mc.gui.hud.getChat().addClientSystemMessage(net.minecraft.network.chat.Component.literal("Movement observer recording: " + path));
        } catch (IOException e) { mc.gui.hud.getChat().addClientSystemMessage(net.minecraft.network.chat.Component.literal("Movement observer start failed: " + e)); }
    }
    private static synchronized void stop(String reason) { if (ACTIVE.compareAndSet(true, false)) stopReason = reason; }

    public static boolean isRecordingFast() { return ACTIVE.get(); }
    public static void packet(Packet<?> p, String direction, String stage, String errorClass) {
        if (!isRecordingFast()) return;
        try {
            JsonObject data = PacketFields.capture(p);
            if (data == null) return;
            data.add("native_id", null); // Minecraft exposes packet type Identifier, not the negotiated numeric wire ID here.
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
            if(p!=null && (packet instanceof net.minecraft.network.protocol.game.ClientboundPlayerPositionPacket || packet instanceof net.minecraft.network.protocol.game.ClientboundPlayerRotationPacket || (packet instanceof net.minecraft.network.protocol.game.ClientboundSetEntityMotionPacket m && m.id()==p.getId()) || (packet instanceof net.minecraft.network.protocol.game.ClientboundTeleportEntityPacket t && t.id()==p.getId()) || (packet instanceof net.minecraft.network.protocol.game.ClientboundUpdateAttributesPacket a && a.getEntityId()==p.getId()))) state.add("player",playerState(p));
            else state.add("player",null);
            if (p!=null && packet instanceof net.minecraft.network.protocol.game.ClientboundUpdateAttributesPacket a && a.getEntityId()==p.getId()) state.add("attributes",attributeState(p,a));
            JsonArray bs=new JsonArray(); blocks.forEach(bs::add); state.add("blocks",bs);
            if(packet instanceof net.minecraft.network.protocol.game.ClientboundBlockChangedAckPacket) { state.add("prediction_state",null); state.addProperty("prediction_state_semantics","Minecraft exposes no local prediction queue snapshot at this hook"); }
            d.add("applied_state",state); offer("inbound",stage,d);
        } catch (RuntimeException ignored) { /* Passive observation cannot change Minecraft packet handling. */ }
    }
    private static JsonObject playerState(LocalPlayer p) {
        JsonObject o=new JsonObject(); o.add("position",vec(p.position())); o.add("velocity",vec(p.getDeltaMovement()));
        o.addProperty("on_ground",p.onGround()); o.addProperty("horizontal_collision",p.horizontalCollision); o.addProperty("yaw",(double)p.getYRot()); o.addProperty("pitch",(double)p.getXRot());
        o.addProperty("sprinting",p.isSprinting()); o.addProperty("crouching",p.isCrouching()); o.addProperty("swimming",p.isSwimming()); o.addProperty("in_water",p.isInWater()); o.addProperty("in_lava",p.isInLava()); o.addProperty("pose",p.getPose().name()); o.addProperty("food_level",p.getFoodData().getFoodLevel());
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
    public static void tick(LocalPlayer p, String stage) {
        if (!ACTIVE.get()) return;
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
        offer("local", stage, d);
    }
    private static JsonObject vec(Vec3 v) { JsonObject a = new JsonObject(); a.addProperty("x",v.x); a.addProperty("y",v.y); a.addProperty("z",v.z); return a; }
    private static JsonObject box(net.minecraft.world.phys.AABB b) { JsonObject a = new JsonObject(); a.add("min", vec(new Vec3(b.minX,b.minY,b.minZ))); a.add("max", vec(new Vec3(b.maxX,b.maxY,b.maxZ))); return a; }
    private static synchronized void offer(String direction, String stage, JsonObject data) {
        long seq = SEQ.incrementAndGet();
        JsonObject r = new JsonObject(); r.addProperty("seq",seq); r.addProperty("offset_us",(System.nanoTime()-startedNanos)/1000); r.addProperty("direction",direction); r.addProperty("stage",stage); r.add("data",data);
        String line = r.toString(); if (line.getBytes(StandardCharsets.UTF_8).length > MAX_ROW || !ROWS.offer(line)) DROPPED.incrementAndGet();
    }
    private static void writeFile(Path file, Minecraft mc) {
        long bytes=0, written=0, oversize=0; String error=null;
        try (BufferedWriter out=Files.newBufferedWriter(file, StandardCharsets.UTF_8, StandardOpenOption.CREATE_NEW)) {
            JsonObject h=new JsonObject(); h.addProperty("type","header"); h.addProperty("schema",1); h.addProperty("utc_start_unix_ms",startWall); h.addProperty("wire_protocol",776); h.addProperty("client_kind","fabric"); h.add("executable", identity(ProcessHandle.current().info().command().orElse(null))); h.add("minecraft_artifact", null); h.addProperty("source_build_revision", buildRevision()); h.addProperty("queue_capacity",QUEUE); h.addProperty("size_limit_bytes",LIMIT); h.addProperty("max_row_bytes",MAX_ROW); h.addProperty("semantics","client observations only; queued != transport write completion != server acceptance");
            out.write(h.toString()); out.newLine();
            while (ACTIVE.get() || !ROWS.isEmpty()) {
                String row=ROWS.poll(100, java.util.concurrent.TimeUnit.MILLISECONDS);
                if(row!=null){int size=row.getBytes(StandardCharsets.UTF_8).length+1;if(size>MAX_ROW){oversize++;DROPPED.incrementAndGet();}else{out.write(row);out.newLine();bytes+=size;written++;if(bytes>=LIMIT)stop("size_limit");}}
            }
            JsonObject f=new JsonObject(); f.addProperty("type","footer"); f.addProperty("utc_end_unix_ms",System.currentTimeMillis()); f.addProperty("offset_us",(System.nanoTime()-startedNanos)/1000); f.addProperty("last_seq",SEQ.get()); f.addProperty("written",written); f.addProperty("dropped",DROPPED.get()); f.addProperty("oversize_omitted",oversize); f.addProperty("reason",stopReason); f.addProperty("complete",DROPPED.get()==0 && oversize==0); out.write(f.toString()); out.newLine(); out.flush();
        } catch(Exception e){ACTIVE.set(false); error=e.toString(); e.printStackTrace();}
        if(mc!=null && mc.gui!=null) { String msg=error==null?"Movement observer saved: "+file+" (dropped="+DROPPED.get()+")":"Movement observer FAILED: "+error+"; "+file; mc.execute(()->mc.gui.hud.getChat().addClientSystemMessage(net.minecraft.network.chat.Component.literal(msg))); }
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
    private MovementObserver() {}
}
