package com.mine_rust.movementobserver.mixin;

import com.mine_rust.movementobserver.MovementObserver;
import net.minecraft.core.BlockPos;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.phys.Vec3;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.Redirect;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(LivingEntity.class)
public abstract class LivingEntityMixin {
    @Shadow private static float computeModifiedFriction(float friction, float modifier) { throw new AssertionError(); }
    @Inject(method="jumpFromGround", at=@At("HEAD"), require=1)
    private void movementobserver$beginJump(CallbackInfo ci) { MovementObserver.beginJump((LivingEntity)(Object)this); }

    @Redirect(method="jumpFromGround", at=@At(value="INVOKE", target="Lnet/minecraft/world/entity/LivingEntity;getJumpPower()F"), require=1)
    private float movementobserver$usedJumpPower(LivingEntity entity) {
        float power = ((LivingEntityInvoker)(Object)this).movementobserver$getJumpPower();
        MovementObserver.jumpPower((LivingEntity)(Object)this, power);
        return power;
    }

    @Redirect(method="getJumpPower(F)F", at=@At(value="INVOKE", target="Lnet/minecraft/world/entity/LivingEntity;getBlockJumpFactor()F"), require=1)
    private float movementobserver$usedBlockJumpFactor(LivingEntity entity) {
        float factor = ((EntityInvoker)(Object)this).movementobserver$getBlockJumpFactor();
        MovementObserver.blockJumpFactor((LivingEntity)(Object)this, factor);
        return factor;
    }

    @Inject(method="jumpFromGround", at=@At("RETURN"), require=1)
    private void movementobserver$endJump(CallbackInfo ci) { MovementObserver.endJump((LivingEntity)(Object)this); }

    @Inject(method="travelInFluid", at=@At("HEAD"), require=1)
    private void movementobserver$beginFluid(Vec3 input, CallbackInfo ci) { MovementObserver.beginPhysics((LivingEntity)(Object)this, ((LivingEntity)(Object)this).isInWater() ? "water" : "lava"); }
    @Inject(method="travelInFluid", at=@At("RETURN"), require=1)
    private void movementobserver$endFluid(Vec3 input, CallbackInfo ci) { MovementObserver.endPhysics((LivingEntity)(Object)this); }
    @Inject(method="travelFallFlying", at=@At("HEAD"), require=1)
    private void movementobserver$beginFlying(Vec3 input, CallbackInfo ci) { MovementObserver.beginPhysics((LivingEntity)(Object)this, "fall_flying"); }
    @Inject(method="travelFallFlying", at=@At("RETURN"), require=1)
    private void movementobserver$endFlying(Vec3 input, CallbackInfo ci) { MovementObserver.endPhysics((LivingEntity)(Object)this); }
    @Redirect(method="travelInFluid", at=@At(value="INVOKE", target="Lnet/minecraft/world/entity/LivingEntity;getEffectiveGravity()D"), require=1)
    private double movementobserver$fluidGravity(LivingEntity entity) { double v=((LivingEntityInvoker)(Object)this).movementobserver$getEffectiveGravity(); MovementObserver.physicsGravity((LivingEntity)(Object)this,v); return v; }
    @Redirect(method="travelInAir", at=@At(value="INVOKE", target="Lnet/minecraft/world/entity/LivingEntity;getEffectiveGravity()D"), require=1)
    private double movementobserver$airGravity(LivingEntity entity) { double v=((LivingEntityInvoker)(Object)this).movementobserver$getEffectiveGravity(); MovementObserver.physicsGravity((LivingEntity)(Object)this,v); return v; }
    @Redirect(method="updateFallFlyingMovement", at=@At(value="INVOKE", target="Lnet/minecraft/world/entity/LivingEntity;getEffectiveGravity()D"), require=1)
    private double movementobserver$flyingGravity(LivingEntity entity) { double v=((LivingEntityInvoker)(Object)this).movementobserver$getEffectiveGravity(); MovementObserver.physicsGravity((LivingEntity)(Object)this,v); return v; }
    @Redirect(method="travelInWater", at=@At(value="INVOKE", target="Lnet/minecraft/world/entity/LivingEntity;moveRelative(FLnet/minecraft/world/phys/Vec3;)V"), require=1)
    private void movementobserver$waterSpeed(LivingEntity entity,float speed,Vec3 input) { MovementObserver.physicsSpeed((LivingEntity)(Object)this,speed); ((EntityInvoker)(Object)this).movementobserver$moveRelative(speed,input); }
    @Redirect(method="travelInLava", at=@At(value="INVOKE", target="Lnet/minecraft/world/entity/LivingEntity;moveRelative(FLnet/minecraft/world/phys/Vec3;)V"), require=1)
    private void movementobserver$lavaSpeed(LivingEntity entity,float speed,Vec3 input) { MovementObserver.physicsSpeed((LivingEntity)(Object)this,speed); ((EntityInvoker)(Object)this).movementobserver$moveRelative(speed,input); }
    @Redirect(method="travelInWater", at=@At(value="INVOKE", target="Lnet/minecraft/world/entity/LivingEntity;getFluidFallingAdjustedMovement(DZLnet/minecraft/world/phys/Vec3;)Lnet/minecraft/world/phys/Vec3;"), require=1)
    private Vec3 movementobserver$waterGravity(LivingEntity entity,double gravity,boolean falling,Vec3 movement) { MovementObserver.physicsAdjustedGravity((LivingEntity)(Object)this,gravity); return entity.getFluidFallingAdjustedMovement(gravity,falling,movement); }
    @Redirect(method="travelInLava", at=@At(value="INVOKE", target="Lnet/minecraft/world/entity/LivingEntity;getFluidFallingAdjustedMovement(DZLnet/minecraft/world/phys/Vec3;)Lnet/minecraft/world/phys/Vec3;"), require=1)
    private Vec3 movementobserver$lavaGravity(LivingEntity entity,double gravity,boolean falling,Vec3 movement) { MovementObserver.physicsAdjustedGravity((LivingEntity)(Object)this,gravity); return entity.getFluidFallingAdjustedMovement(gravity,falling,movement); }
    @Redirect(method="travelInWater", at=@At(value="INVOKE", target="Lnet/minecraft/world/phys/Vec3;multiply(DDD)Lnet/minecraft/world/phys/Vec3;"), require=1)
    private Vec3 movementobserver$waterDrag(Vec3 value,double x,double y,double z) { MovementObserver.physicsDrag((LivingEntity)(Object)this,x,y,z); return value.multiply(x,y,z); }
    @Redirect(method="travelInLava", at=@At(value="INVOKE", target="Lnet/minecraft/world/phys/Vec3;multiply(DDD)Lnet/minecraft/world/phys/Vec3;"), require=1)
    private Vec3 movementobserver$lavaDrag(Vec3 value,double x,double y,double z) { MovementObserver.physicsDrag((LivingEntity)(Object)this,x,y,z); return value.multiply(x,y,z); }
    @Redirect(method="travelInLava", at=@At(value="INVOKE", target="Lnet/minecraft/world/phys/Vec3;scale(D)Lnet/minecraft/world/phys/Vec3;"), require=1)
    private Vec3 movementobserver$lavaScale(Vec3 value,double scale) { MovementObserver.physicsScale((LivingEntity)(Object)this,scale); return value.scale(scale); }
    @Redirect(method="updateFallFlyingMovement", at=@At(value="INVOKE", target="Lnet/minecraft/world/phys/Vec3;multiply(DDD)Lnet/minecraft/world/phys/Vec3;"), require=1)
    private Vec3 movementobserver$flyingDrag(Vec3 value,double x,double y,double z) { MovementObserver.physicsDrag((LivingEntity)(Object)this,x,y,z); return value.multiply(x,y,z); }
    @Inject(method="travelInAir", at=@At("HEAD"), require=1)
    private void movementobserver$beginAirTravel(Vec3 input, CallbackInfo ci) { MovementObserver.beginAirTravel((LivingEntity)(Object)this); }

    @Redirect(method="travelInAir", at=@At(value="INVOKE", target="Lnet/minecraft/world/entity/LivingEntity;getBlockPosBelowThatAffectsMyMovement()Lnet/minecraft/core/BlockPos;"), require=1)
    private BlockPos movementobserver$usedFrictionSource(LivingEntity entity) {
        BlockPos pos = entity.getBlockPosBelowThatAffectsMyMovement();
        MovementObserver.airFrictionSource((LivingEntity)(Object)this, pos);
        return pos;
    }

    @Redirect(method="travelInAir", at=@At(value="INVOKE", target="Lnet/minecraft/world/entity/LivingEntity;computeModifiedFriction(FF)F", ordinal=1), require=1)
    private float movementobserver$usedAirDrag(float friction, float modifier) {
        float drag = computeModifiedFriction(friction, modifier);
        MovementObserver.airGroundDrag((net.minecraft.world.entity.LivingEntity)(Object)this, drag);
        return drag;
    }

    @Redirect(method="travelInAir", at=@At(value="INVOKE", target="Lnet/minecraft/world/entity/LivingEntity;handleRelativeFrictionAndCalculateMovement(Lnet/minecraft/world/phys/Vec3;F)Lnet/minecraft/world/phys/Vec3;"), require=1)
    private Vec3 movementobserver$usedBlockFriction(LivingEntity entity, Vec3 input, float friction) {
        MovementObserver.airBlockFriction((LivingEntity)(Object)this, friction);
        return ((LivingEntityInvoker)(Object)this).movementobserver$handleRelativeFrictionAndCalculateMovement(input, friction);
    }

    @Inject(method="travelInAir", at=@At("RETURN"), require=1)
    private void movementobserver$endAirTravel(Vec3 input, CallbackInfo ci) { MovementObserver.endAirTravel((LivingEntity)(Object)this); }
}
