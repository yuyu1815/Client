package com.mine_rust.movementobserver.mixin;

import com.llamalad7.mixinextras.injector.wrapmethod.WrapMethod;
import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.mine_rust.movementobserver.MovementObserver;
import io.netty.buffer.ByteBuf;
import io.netty.channel.ChannelHandlerContext;
import net.minecraft.network.PacketDecoder;
import net.minecraft.network.ProtocolInfo;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import java.util.List;

@Mixin(PacketDecoder.class)
public abstract class PacketDecoderMixin {
    @Shadow @Final private ProtocolInfo<?> protocolInfo;

    @WrapMethod(method = "decode")
    private void movementobserver$capture(ChannelHandlerContext ctx, ByteBuf input, List<Object> output, Operation<Void> original) throws Exception {
        MovementObserver.beginRaw(ctx.channel(), "inbound", protocolInfo.id().name(), null, input.readerIndex());
        try { original.call(ctx, input, output); }
        finally {
            String packetType = output.isEmpty() || !(output.get(output.size() - 1) instanceof net.minecraft.network.protocol.Packet<?> packet) ? null : packet.type().id().toString();
            MovementObserver.endRaw(ctx.channel(), input, input.readerIndex(), packetType);
        }
    }
}
