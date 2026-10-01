package com.mine_rust.movementobserver.mixin;

import com.mine_rust.movementobserver.MovementObserver;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientPacketListener;
import net.minecraft.core.BlockPos;
import net.minecraft.network.protocol.game.*;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

import java.util.ArrayList;
import java.util.List;

/** Passive head/tail observations. Off-thread PacketUtils dispatch calls are ignored at head. */
@Mixin(ClientPacketListener.class)
public abstract class ClientPacketListenerMixin {
    private static boolean movementobserver$mainThread() { return Minecraft.getInstance().isSameThread(); }
    @Inject(method="handleMovePlayer", at=@At("HEAD")) private void mo$positionBefore(ClientboundPlayerPositionPacket p, CallbackInfo ci){if(movementobserver$mainThread())MovementObserver.applied(p,"apply_before",List.of());}
    @Inject(method="handleMovePlayer", at=@At("TAIL")) private void mo$positionAfter(ClientboundPlayerPositionPacket p, CallbackInfo ci){if(movementobserver$mainThread())MovementObserver.applied(p,"apply_after",List.of());}
    @Inject(method="handleRotatePlayer", at=@At("HEAD")) private void mo$rotationBefore(ClientboundPlayerRotationPacket p, CallbackInfo ci){if(movementobserver$mainThread())MovementObserver.applied(p,"apply_before",List.of());}
    @Inject(method="handleRotatePlayer", at=@At("TAIL")) private void mo$rotationAfter(ClientboundPlayerRotationPacket p, CallbackInfo ci){if(movementobserver$mainThread())MovementObserver.applied(p,"apply_after",List.of());}
    @Inject(method="handleSetEntityMotion", at=@At("HEAD")) private void mo$motionBefore(ClientboundSetEntityMotionPacket p, CallbackInfo ci){if(movementobserver$mainThread()&&movementobserver$own(p.id()))MovementObserver.applied(p,"apply_before",List.of());}
    @Inject(method="handleSetEntityMotion", at=@At("TAIL")) private void mo$motionAfter(ClientboundSetEntityMotionPacket p, CallbackInfo ci){if(movementobserver$mainThread()&&movementobserver$own(p.id()))MovementObserver.applied(p,"apply_after",List.of());}
    @Inject(method="handleTeleportEntity", at=@At("HEAD")) private void mo$teleportBefore(ClientboundTeleportEntityPacket p, CallbackInfo ci){if(movementobserver$mainThread()&&movementobserver$own(p.id()))MovementObserver.applied(p,"apply_before",List.of());}
    @Inject(method="handleTeleportEntity", at=@At("TAIL")) private void mo$teleportAfter(ClientboundTeleportEntityPacket p, CallbackInfo ci){if(movementobserver$mainThread()&&movementobserver$own(p.id()))MovementObserver.applied(p,"apply_after",List.of());}
    @Inject(method="handleEntityPositionSync", at=@At("HEAD")) private void mo$positionSyncBefore(ClientboundEntityPositionSyncPacket p, CallbackInfo ci){if(movementobserver$mainThread()&&movementobserver$own(p.id()))MovementObserver.applied(p,"apply_before",List.of());}
    @Inject(method="handleEntityPositionSync", at=@At("TAIL")) private void mo$positionSyncAfter(ClientboundEntityPositionSyncPacket p, CallbackInfo ci){if(movementobserver$mainThread()&&movementobserver$own(p.id()))MovementObserver.applied(p,"apply_after",List.of());}
    @Inject(method="handleBlockUpdate", at=@At("HEAD")) private void mo$blockBefore(ClientboundBlockUpdatePacket p, CallbackInfo ci){if(movementobserver$mainThread())MovementObserver.applied(p,"apply_before",List.of(MovementObserver.blockState(p.getPos())));}
    @Inject(method="handleBlockUpdate", at=@At("TAIL")) private void mo$blockAfter(ClientboundBlockUpdatePacket p, CallbackInfo ci){if(movementobserver$mainThread())MovementObserver.applied(p,"apply_after",List.of(MovementObserver.blockState(p.getPos())));}
    @Inject(method="handleChunkBlocksUpdate", at=@At("HEAD")) private void mo$sectionBefore(ClientboundSectionBlocksUpdatePacket p, CallbackInfo ci){if(movementobserver$mainThread())MovementObserver.applied(p,"apply_before",MovementObserver.sectionStates(p));}
    @Inject(method="handleChunkBlocksUpdate", at=@At("TAIL")) private void mo$sectionAfter(ClientboundSectionBlocksUpdatePacket p, CallbackInfo ci){if(movementobserver$mainThread())MovementObserver.applied(p,"apply_after",MovementObserver.sectionStates(p));}
    @Inject(method="handleBlockChangedAck", at=@At("HEAD")) private void mo$ackBefore(ClientboundBlockChangedAckPacket p, CallbackInfo ci){if(movementobserver$mainThread())MovementObserver.applied(p,"apply_before",List.of());}
    @Inject(method="handleBlockChangedAck", at=@At("TAIL")) private void mo$ackAfter(ClientboundBlockChangedAckPacket p, CallbackInfo ci){if(movementobserver$mainThread())MovementObserver.applied(p,"apply_after",List.of());}
    @Inject(method="handleUpdateAttributes", at=@At("HEAD")) private void mo$attributesBefore(ClientboundUpdateAttributesPacket p, CallbackInfo ci){if(movementobserver$mainThread()&&movementobserver$own(p.getEntityId()))MovementObserver.applied(p,"apply_before",List.of());}
    @Inject(method="handleUpdateAttributes", at=@At("TAIL")) private void mo$attributesAfter(ClientboundUpdateAttributesPacket p, CallbackInfo ci){if(movementobserver$mainThread()&&movementobserver$own(p.getEntityId()))MovementObserver.applied(p,"apply_after",List.of());}
    private static boolean movementobserver$own(int id){Entity p=Minecraft.getInstance().player;return p!=null&&p.getId()==id;}
}
