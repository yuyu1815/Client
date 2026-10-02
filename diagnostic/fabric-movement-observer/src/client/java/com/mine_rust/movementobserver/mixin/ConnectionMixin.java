package com.mine_rust.movementobserver.mixin;

import com.mine_rust.movementobserver.MovementObserver;
import com.mine_rust.movementobserver.PacketWriteObserver;
import io.netty.channel.ChannelDuplexHandler;
import io.netty.channel.ChannelHandlerContext;
import net.minecraft.network.Connection;
import net.minecraft.network.protocol.Packet;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(Connection.class)
public abstract class ConnectionMixin {
    @Shadow private net.minecraft.network.protocol.PacketFlow receiving;

    @Inject(method = "channelActive", at = @At("TAIL"))
    private void movementobserver$install(ChannelHandlerContext ctx, CallbackInfo ci) {
        String name = "movementobserver_observer";
        if (ctx.pipeline().get(name) != null) return;
        ctx.pipeline().addBefore(ctx.name(), name, new ChannelDuplexHandler() {
            @Override public void channelRead(ChannelHandlerContext context, Object msg) throws Exception {
                String traceId = context.channel().attr(com.mine_rust.movementobserver.PacketTrace.KEY).get();
                try {
                    if (MovementObserver.isRecordingFast() && msg instanceof Packet<?> packet && receiving == net.minecraft.network.protocol.PacketFlow.CLIENTBOUND)
                        MovementObserver.packet(packet, "inbound", "received", null, traceId);
                    super.channelRead(context, msg);
                } finally { context.channel().attr(com.mine_rust.movementobserver.PacketTrace.KEY).set(null); }
            }
            @Override public void write(ChannelHandlerContext context, Object msg, io.netty.channel.ChannelPromise promise) throws Exception {
                PacketWriteObserver.write(context, msg, promise,
                        PacketWriteObserver.shouldObserveOutbound(MovementObserver.isRecordingFast(), receiving),
                        (packet, stage, cause, traceId) -> MovementObserver.packet(packet, "outbound", stage,
                                cause == null ? (stage.equals("transport_write_failure") ? "unknown" : null) : cause.getClass().getName(), traceId));
            }
        });
    }
}
