package com.mine_rust.movementobserver.mixin;

import com.llamalad7.mixinextras.injector.wrapmethod.WrapMethod;
import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.mine_rust.movementobserver.MovementObserver;
import io.netty.buffer.ByteBuf;
import io.netty.channel.ChannelHandlerContext;
import net.minecraft.network.PacketEncoder;
import net.minecraft.network.ProtocolInfo;
import net.minecraft.network.protocol.Packet;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;

@Mixin(PacketEncoder.class)
public abstract class PacketEncoderMixin {
    @Shadow @Final private ProtocolInfo<?> protocolInfo;

    @WrapMethod(method = "encode(Lio/netty/channel/ChannelHandlerContext;Lnet/minecraft/network/protocol/Packet;Lio/netty/buffer/ByteBuf;)V")
    private void movementobserver$capture(ChannelHandlerContext ctx, Packet<?> packet, ByteBuf out, Operation<Void> original) throws Exception {
        MovementObserver.beginRaw(ctx.channel(), "outbound", protocolInfo.id().name(), packet.type().id().toString(), out.writerIndex());
        try { original.call(ctx, packet, out); }
        finally { MovementObserver.endRaw(ctx.channel(), out, out.writerIndex()); }
    }
}
