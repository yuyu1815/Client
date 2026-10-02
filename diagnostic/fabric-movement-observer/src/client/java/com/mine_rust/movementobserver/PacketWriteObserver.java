package com.mine_rust.movementobserver;

import io.netty.channel.ChannelHandlerContext;
import io.netty.channel.ChannelPromise;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.PacketFlow;
import java.util.UUID;

/** Observes the actual channel write promise without changing the packet or writing it twice. */
public final class PacketWriteObserver {
    @FunctionalInterface
    public interface Event {
        void accept(Packet<?> packet, String stage, Throwable cause, String traceId);
    }

    private PacketWriteObserver() {}

    public static boolean shouldObserveOutbound(boolean recording, PacketFlow receiving) {
        return recording && receiving == PacketFlow.CLIENTBOUND;
    }

    public static void write(ChannelHandlerContext context, Object message, ChannelPromise promise, boolean observe, Event event) throws Exception {
        if (!observe || !(message instanceof Packet<?> packet)) {
            context.write(message, promise);
            return;
        }
        String traceId = UUID.randomUUID().toString();
        String previous = context.channel().attr(PacketTrace.KEY).getAndSet(traceId);
        event.accept(packet, "transport_write_attempt", null, traceId);
        ChannelPromise observed = promise.isVoid() ? context.newPromise() : promise;
        observed.addListener(future -> event.accept(packet,
                future.isSuccess() ? "transport_write_success" : "transport_write_failure",
                future.cause(), traceId));
        try { context.write(message, observed); }
        finally { context.channel().attr(PacketTrace.KEY).set(previous); }
    }
}
