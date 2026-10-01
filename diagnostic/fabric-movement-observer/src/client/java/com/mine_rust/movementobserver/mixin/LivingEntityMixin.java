package com.mine_rust.movementobserver.mixin;

import com.mine_rust.movementobserver.MovementObserver;
import net.minecraft.core.BlockPos;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.phys.Vec3;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.Redirect;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(LivingEntity.class)
public abstract class LivingEntityMixin {
    @Inject(method="jumpFromGround", at=@At("HEAD"), require=1)
    private void movementobserver$beginJump(CallbackInfo ci) { MovementObserver.beginJump((LivingEntity)(Object)this); }

    @Redirect(method="jumpFromGround", at=@At(value="INVOKE", target="Lnet/minecraft/world/entity/LivingEntity;getJumpPower()F"), require=1)
    private float movementobserver$usedJumpPower(LivingEntity entity) {
        float power = ((LivingEntityInvoker)(Object)this).movementobserver$getJumpPower();
        MovementObserver.jumpPower((LivingEntity)(Object)this, power);
        return power;
    }

    @Redirect(method="getJumpPower(F)F", at=@At(value="INVOKE", target="Lnet/minecraft/world/entity/Entity;getBlockJumpFactor()F"), require=1)
    private float movementobserver$usedBlockJumpFactor(Entity entity) {
        float factor = ((EntityInvoker)(Object)this).movementobserver$getBlockJumpFactor();
        MovementObserver.blockJumpFactor((LivingEntity)(Object)this, factor);
        return factor;
    }

    @Inject(method="jumpFromGround", at=@At("RETURN"), require=1)
    private void movementobserver$endJump(CallbackInfo ci) { MovementObserver.endJump((LivingEntity)(Object)this); }

    @Inject(method="travelInAir", at=@At("HEAD"), require=1)
    private void movementobserver$beginAirTravel(Vec3 input, CallbackInfo ci) { MovementObserver.beginAirTravel((LivingEntity)(Object)this); }

    @Redirect(method="travelInAir", at=@At(value="INVOKE", target="Lnet/minecraft/world/entity/Entity;getBlockPosBelowThatAffectsMyMovement()Lnet/minecraft/core/BlockPos;"), require=1)
    private BlockPos movementobserver$usedFrictionSource(Entity entity) {
        BlockPos pos = entity.getBlockPosBelowThatAffectsMyMovement();
        MovementObserver.airFrictionSource((LivingEntity)(Object)this, pos);
        return pos;
    }

    @Redirect(method="travelInAir", at=@At(value="INVOKE", target="Lnet/minecraft/world/entity/LivingEntity;handleRelativeFrictionAndCalculateMovement(Lnet/minecraft/world/phys/Vec3;F)Lnet/minecraft/world/phys/Vec3;"), require=1)
    private Vec3 movementobserver$usedBlockFriction(LivingEntity entity, Vec3 input, float friction) {
        MovementObserver.airBlockFriction((LivingEntity)(Object)this, friction);
        return ((LivingEntityInvoker)(Object)this).movementobserver$handleRelativeFrictionAndCalculateMovement(input, friction);
    }

    @Inject(method="travelInAir", at=@At("RETURN"), require=1)
    private void movementobserver$endAirTravel(Vec3 input, CallbackInfo ci) { MovementObserver.endAirTravel((LivingEntity)(Object)this); }
}
