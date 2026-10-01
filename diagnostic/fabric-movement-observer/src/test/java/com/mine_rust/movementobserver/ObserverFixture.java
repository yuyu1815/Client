package com.mine_rust.movementobserver;

import com.google.gson.JsonObject;
import io.netty.channel.ChannelHandlerContext;
import io.netty.channel.ChannelOutboundHandlerAdapter;
import io.netty.channel.ChannelPromise;
import io.netty.channel.embedded.EmbeddedChannel;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.game.ClientboundEntityPositionSyncPacket;
import net.minecraft.network.protocol.game.ServerboundAcceptTeleportationPacket;
import net.minecraft.network.protocol.game.ServerboundMovePlayerPacket;
import net.minecraft.network.protocol.game.ServerboundPlayerInputPacket;
import net.minecraft.world.entity.PositionMoveRotation;
import net.minecraft.world.entity.player.Input;
import net.minecraft.world.phys.Vec3;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.EmptyBlockGetter;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.phys.shapes.CollisionContext;
import net.minecraft.world.phys.shapes.Shapes;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.atomic.AtomicInteger;

/** Framework-free production packet and promise fixture, run by packetFieldsSmoke. */
public final class ObserverFixture {
    public static void main(String[] args) {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        packets();
        promises();
        collisionShapes();
        System.out.println("observer fixture: PASS (bootstrapped block collision/outline shapes, negative-coordinate transform, box cap/privacy/status; typed packets and promises)");
    }

    private static void collisionShapes() {
        var pos = new BlockPos(-3, 7, -5);
        var empty = EmptyBlockGetter.INSTANCE;
        var context = CollisionContext.empty();
        var chest = Blocks.CHEST.defaultBlockState().getCollisionShape(empty, pos, context);
        var soul = Blocks.SOUL_SAND.defaultBlockState();
        var honey = Blocks.HONEY_BLOCK.defaultBlockState().getCollisionShape(empty, pos, context);
        assert Math.abs(chest.max(net.minecraft.core.Direction.Axis.Y) - 0.875) < 1e-9;
        assert Math.abs(soul.getCollisionShape(empty, pos, context).max(net.minecraft.core.Direction.Axis.Y) - 0.875) < 1e-9;
        assert Math.abs(soul.getShape(empty, pos, context).max(net.minecraft.core.Direction.Axis.Y) - 1.0) < 1e-9;
        assert Math.abs(honey.max(net.minecraft.core.Direction.Axis.Y) - 0.9375) < 1e-9;
        var localChest = chest.toAabbs().get(0);
        var translated = CollisionSnapshots.worldBox(localChest, -3, 7, -5);
        assert translated.minX == localChest.minX - 3 && translated.minY == localChest.minY + 7 && translated.minZ == localChest.minZ - 5;
        assert translated.minX < 0 && translated.minZ < 0;
        var many = Shapes.or(Shapes.box(0,0,0,.1,.1,.1), Shapes.box(.2,0,0,.3,.1,.1), Shapes.box(.4,0,0,.5,.1,.1), Shapes.box(.6,0,0,.7,.1,.1), Shapes.box(.8,0,0,.9,.1,.1));
        assert CollisionSnapshots.boundedBoxes(many.toAabbs(), 4).size() == 4;
        assert !CollisionSnapshots.finite(new net.minecraft.world.phys.AABB(Double.NaN,0,0,1,1,1));
        var emptySnapshot = CollisionSnapshots.capture(null, new net.minecraft.world.phys.AABB(-1, 0, -1, 1, 2, 1), Vec3.ZERO);
        assert emptySnapshot.get("status").getAsString().equals("null_world");
        assert emptySnapshot.get("max_block_cells").getAsInt() == 32;
        assert emptySnapshot.get("max_boxes").getAsInt() == 128;
        assert emptySnapshot.get("visited_cells").getAsInt() == 0;
        assert emptySnapshot.get("block_cells").isJsonArray() && emptySnapshot.get("shape_aabbs").isJsonArray();
        assert emptySnapshot.getAsJsonObject("visited_range").getAsJsonArray("min").get(0).getAsInt() == -1;
        assert !emptySnapshot.toString().contains("custom_name") && !emptySnapshot.toString().contains("uuid") && !emptySnapshot.toString().contains("nbt") && !emptySnapshot.toString().contains("chat");
    }

    private static void packets() {
        JsonObject posRot = PacketFields.capture(new ServerboundMovePlayerPacket.PosRot(
                new Vec3(1.25, 64.0, -8.5), 90.0f, -10.0f, true, false));
        assert posRot.get("packet").getAsString().equals("move_player_pos_rot");
        assert posRot.getAsJsonObject("fields").getAsJsonArray("position").get(0).getAsDouble() == 1.25;
        assert posRot.getAsJsonObject("fields").getAsJsonArray("yaw_pitch").get(0).getAsDouble() == 90.0;
        assert posRot.getAsJsonObject("fields").get("on_ground").getAsBoolean();

        JsonObject input = PacketFields.capture(new ServerboundPlayerInputPacket(
                new Input(true, false, true, false, true, false, true)));
        assert input.get("packet").getAsString().equals("player_input");
        assert input.getAsJsonObject("fields").get("forward").getAsBoolean();
        assert input.getAsJsonObject("fields").get("jump").getAsBoolean();
        assert input.getAsJsonObject("fields").get("sprint").getAsBoolean();

        JsonObject correction = PacketFields.capture(new ClientboundEntityPositionSyncPacket(
                4, new PositionMoveRotation(new Vec3(2, 70, 3), new Vec3(0.25, 0, -0.5), 45.0f, 5.0f), true));
        assert correction.get("packet").getAsString().equals("entity_position_sync");
        assert correction.getAsJsonObject("fields").get("entity_id").getAsInt() == 4;
        assert correction.getAsJsonObject("fields").getAsJsonObject("change").getAsJsonArray("position").get(1).getAsDouble() == 70.0;
        assert correction.getAsJsonObject("fields").get("on_ground").getAsBoolean();

        JsonObject teleport = PacketFields.capture(new ServerboundAcceptTeleportationPacket(23));
        assert teleport.get("packet").getAsString().equals("accept_teleportation");
        assert teleport.getAsJsonObject("fields").get("teleport_id").getAsInt() == 23;
    }

    private static void promises() {
        checkWrite(false, true, "transport_write_success");
        checkWrite(false, false, "transport_write_failure");
        checkWrite(true, true, "transport_write_success");
    }

    private static void checkWrite(boolean voidPromise, boolean succeed, String expectedStage) {
        Packet<?> packet = new ServerboundAcceptTeleportationPacket(9);
        RuntimeException failure = new RuntimeException("fixture failure");
        AtomicInteger writes = new AtomicInteger();
        List<String> stages = new ArrayList<>();
        PacketWriteObserver.Event event = (p, stage, cause) -> {
            assert p == packet;
            stages.add(stage);
        };
        ChannelOutboundHandlerAdapter sink = new ChannelOutboundHandlerAdapter() {
            @Override public void write(ChannelHandlerContext ctx, Object msg, ChannelPromise promise) {
                assert msg == packet;
                writes.incrementAndGet();
                if (succeed) ctx.write(msg, promise);
                else promise.setFailure(failure);
            }
        };
        ChannelOutboundHandlerAdapter observer = new ChannelOutboundHandlerAdapter() {
            @Override public void write(ChannelHandlerContext ctx, Object msg, ChannelPromise promise) throws Exception {
                PacketWriteObserver.write(ctx, msg, promise, true, event);
            }
        };
        EmbeddedChannel channel = new EmbeddedChannel(sink, observer);
        ChannelPromise promise = voidPromise ? channel.voidPromise() : channel.newPromise();
        channel.write(packet, promise);
        channel.flushOutbound();
        channel.runPendingTasks();
        assert writes.get() == 1 : writes.get();
        assert stages.size() == 2 : stages;
        assert stages.get(0).equals("transport_write_attempt");
        assert stages.get(1).equals(expectedStage) : stages;
        if (!voidPromise) assert promise.isSuccess() == succeed;
        if (succeed) assert channel.readOutbound() == packet;
        else assert channel.readOutbound() == null;
        channel.finishAndReleaseAll();
    }
}
