package com.mine_rust.movementobserver.mixin;

import com.mine_rust.movementobserver.MovementObserver;
import io.netty.channel.ChannelDuplexHandler;
import io.netty.channel.ChannelHandlerContext;
import io.netty.channel.ChannelPromise;
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
                if (msg instanceof Packet<?> packet && receiving == net.minecraft.network.protocol.PacketFlow.CLIENTBOUND)
                    MovementObserver.packet(packet, "inbound", "received", true);
                super.channelRead(context, msg);
            }
            @Override public void write(ChannelHandlerContext context, Object msg, ChannelPromise promise) throws Exception {
                if (msg instanceof Packet<?> packet && receiving == net.minecraft.network.protocol.PacketFlow.SERVERBOUND) {
                    MovementObserver.packet(packet, "outbound", "transport_write_attempt", true);
                    // Vanilla commonly passes channel.voidPromise(), which rejects listeners.
                    ChannelPromise observed = promise.isVoid() ? context.newPromise() : promise;
                    observed.addListener(f -> MovementObserver.packet(packet, "outbound", "transport_write_complete", f.isSuccess()));
                    super.write(context, msg, observed);
                    return;
                }
                super.write(context, msg, promise);
            }
        });
    }
}
