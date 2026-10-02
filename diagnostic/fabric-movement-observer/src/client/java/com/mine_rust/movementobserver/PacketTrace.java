package com.mine_rust.movementobserver;

import io.netty.util.AttributeKey;

public final class PacketTrace {
    public static final AttributeKey<String> KEY = AttributeKey.valueOf("movementobserver:packet_trace");
    private PacketTrace() {}
}
