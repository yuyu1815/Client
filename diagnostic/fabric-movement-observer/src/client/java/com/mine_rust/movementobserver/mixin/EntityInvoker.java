package com.mine_rust.movementobserver.mixin;

import net.minecraft.world.phys.Vec3;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Invoker;
import net.minecraft.world.entity.Entity;

@Mixin(Entity.class)
public interface EntityInvoker {
    @Invoker("collide") Vec3 movementobserver$collide(Vec3 requested);
    @Invoker("getBlockJumpFactor") float movementobserver$getBlockJumpFactor();
}
