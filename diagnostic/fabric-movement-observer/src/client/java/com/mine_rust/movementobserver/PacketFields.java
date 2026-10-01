package com.mine_rust.movementobserver;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.game.*;
import net.minecraft.world.entity.Relative;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.Vec3;

import java.util.Set;

/** Explicit schema-1 packet allowlist. Never introspects or serializes arbitrary packet data. */
final class PacketFields {
    private PacketFields() {}
    static JsonObject capture(Packet<?> p) {
        JsonObject data = new JsonObject();
        String kind;
        JsonObject f = new JsonObject();
        if (p instanceof ServerboundMovePlayerPacket m) {
            kind = switch (p) { case ServerboundMovePlayerPacket.Pos ignored -> "move_player_pos"; case ServerboundMovePlayerPacket.PosRot ignored -> "move_player_pos_rot"; case ServerboundMovePlayerPacket.Rot ignored -> "move_player_rot"; default -> "move_player_status_only"; };
            f.add("position", m.hasPosition() ? vec(new Vec3(m.getX(0), m.getY(0), m.getZ(0))) : null);
            f.add("yaw_pitch", m.hasRotation() ? pair(m.getYRot(0), m.getXRot(0)) : null);
            f.addProperty("on_ground", m.isOnGround()); f.addProperty("horizontal_collision", m.horizontalCollision());
        } else if (p instanceof ServerboundPlayerInputPacket q) {
            kind="player_input"; var i=q.input(); f.addProperty("forward",i.forward()); f.addProperty("backward",i.backward()); f.addProperty("left",i.left()); f.addProperty("right",i.right()); f.addProperty("jump",i.jump()); f.addProperty("shift",i.shift()); f.addProperty("sprint",i.sprint());
        } else if (p instanceof ServerboundPlayerCommandPacket q) {
            kind="player_command"; f.addProperty("entity_id",q.getId()); f.addProperty("action",q.getAction().name().toLowerCase(java.util.Locale.ROOT)); f.addProperty("action_code",q.getAction().ordinal()); f.addProperty("data",q.getData()); if(q.getAction()==ServerboundPlayerCommandPacket.Action.START_RIDING_JUMP) f.addProperty("jump_strength",q.getData());
        } else if (p instanceof ServerboundUseItemOnPacket q) {
            kind="use_item_on"; BlockHitResult h=q.getHitResult(); f.addProperty("hand",q.getHand().ordinal()); f.addProperty("sequence",q.getSequence()); f.add("block",pos(h.getBlockPos())); f.addProperty("face",h.getDirection().get3DDataValue()); f.add("hit",vec(h.getLocation())); f.addProperty("inside",h.isInside()); f.addProperty("world_border",h.isWorldBorderHit());
        } else if (p instanceof ServerboundUseItemPacket q) {
            kind="use_item"; f.addProperty("hand",q.getHand().ordinal()); f.addProperty("sequence",q.getSequence()); f.add("yaw_pitch",pair(q.getYRot(),q.getXRot()));
        } else if (p instanceof ServerboundPlayerActionPacket q) {
            kind="player_action"; f.addProperty("action",q.getAction().ordinal()); f.add("block",pos(q.getPos())); f.addProperty("face",q.getDirection().get3DDataValue()); f.addProperty("sequence",q.getSequence());
        } else if (p instanceof ServerboundSwingPacket q) { kind="swing"; f.addProperty("hand",q.getHand().ordinal());
        } else if (p instanceof ServerboundAcceptTeleportationPacket q) { kind="accept_teleportation"; f.addProperty("teleport_id",q.getId());
        } else if (p instanceof ClientboundPlayerPositionPacket q) {
            kind="player_position"; f.addProperty("teleport_id",q.id()); var c=q.change(); f.add("change", change(c)); f.add("relative", relative(q.relatives()));
        } else if (p instanceof ClientboundPlayerRotationPacket q) {
            kind="player_rotation"; f.add("yaw_pitch",pair(q.yRot(),q.xRot())); f.add("relative",pair(q.relativeY(),q.relativeX()));
        } else if (p instanceof ClientboundSetEntityMotionPacket q) { kind="set_entity_motion"; f.addProperty("entity_id",q.id()); f.add("velocity",vec(q.movement()));
        } else if (p instanceof ClientboundTeleportEntityPacket q) {
            kind="teleport_entity"; f.addProperty("entity_id",q.id()); f.add("change",change(q.change())); f.add("relative",relative(q.relatives())); f.addProperty("on_ground",q.onGround());
        } else if (p instanceof ClientboundBlockUpdatePacket q) {
            kind="block_update"; f.add("block",pos(q.getPos())); f.addProperty("state",net.minecraft.world.level.block.Block.BLOCK_STATE_REGISTRY.getId(q.getBlockState())); f.addProperty("block_name",BuiltInRegistries.BLOCK.getKey(q.getBlockState().getBlock()).toString());
        } else if (p instanceof ClientboundSectionBlocksUpdatePacket q) {
            kind="section_blocks_update"; JsonArray updates=new JsonArray(); int[] count={0}; q.runUpdates((bp,bs)->{ if(count[0]++<4096){JsonObject u=new JsonObject();u.add("block",pos(bp));u.addProperty("state",net.minecraft.world.level.block.Block.BLOCK_STATE_REGISTRY.getId(bs));u.addProperty("block_name",BuiltInRegistries.BLOCK.getKey(bs.getBlock()).toString());updates.add(u);} }); f.add("updates",updates); f.addProperty("count",count[0]); f.addProperty("truncated",count[0]>4096);
        } else if (p instanceof ClientboundBlockChangedAckPacket q) { kind="block_ack"; f.addProperty("sequence",q.sequence());
        } else if (p instanceof ClientboundUpdateAttributesPacket q) {
            kind="own_attribute"; f.addProperty("entity_id",q.getEntityId()); JsonArray attrs=new JsonArray();
            for (var a:q.getValues()) { JsonObject x=new JsonObject(); x.addProperty("attribute",a.attribute().unwrapKey().map(k->k.identifier().toString()).orElse("unknown")); x.addProperty("base",a.base()); JsonArray mods=new JsonArray(); for(var m:a.modifiers()){JsonObject z=new JsonObject();z.addProperty("id",m.id().toString());z.addProperty("amount",m.amount());z.addProperty("operation",m.operation().name().toLowerCase(java.util.Locale.ROOT));mods.add(z);} x.add("modifiers",mods); attrs.add(x); } f.add("attributes",attrs);
        } else return null;
        data.addProperty("packet",kind); data.add("fields",f); return data;
    }
    private static JsonArray vec(Vec3 v){JsonArray o=new JsonArray();o.add(v.x);o.add(v.y);o.add(v.z);return o;}
    private static JsonArray pair(float a,float b){JsonArray x=new JsonArray();x.add((double)a);x.add((double)b);return x;}
    private static JsonArray pair(boolean a,boolean b){JsonArray x=new JsonArray();x.add(a);x.add(b);return x;}
    private static JsonArray pos(BlockPos p){JsonArray x=new JsonArray();x.add(p.getX());x.add(p.getY());x.add(p.getZ());return x;}
    private static JsonObject change(net.minecraft.world.entity.PositionMoveRotation c){JsonObject o=new JsonObject();o.add("position",vec(c.position()));o.add("velocity",vec(c.deltaMovement()));o.add("yaw_pitch",pair(c.yRot(),c.xRot()));return o;}
    private static JsonObject relative(Set<Relative> rs){JsonObject o=new JsonObject();o.addProperty("flags",Relative.pack(rs));o.addProperty("rotate_delta",rs.contains(Relative.ROTATE_DELTA));return o;}
}
