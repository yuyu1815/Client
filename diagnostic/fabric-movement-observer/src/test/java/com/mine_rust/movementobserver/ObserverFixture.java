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

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.atomic.AtomicInteger;

/** Framework-free production packet and promise fixture, run by packetFieldsSmoke. */
public final class ObserverFixture {
    public static void main(String[] args) {
        packets();
        promises();
        System.out.println("observer fixture: PASS (typed packets; success/failure/void promise; one unchanged message write)");
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
