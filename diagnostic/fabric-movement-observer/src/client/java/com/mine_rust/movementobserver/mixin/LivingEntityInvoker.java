package com.mine_rust.movementobserver.mixin;

import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.phys.Vec3;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Invoker;

@Mixin(LivingEntity.class)
public interface LivingEntityInvoker {
    @Invoker("handleRelativeFrictionAndCalculateMovement") Vec3 movementobserver$handleRelativeFrictionAndCalculateMovement(Vec3 input, float friction);
    @Invoker("getJumpPower") float movementobserver$getJumpPower();
    @Invoker("getEffectiveGravity") double movementobserver$getEffectiveGravity();
}
