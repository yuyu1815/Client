package com.mine_rust.movementobserver.mixin;

import com.mine_rust.movementobserver.MovementObserver;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.MoverType;
import net.minecraft.world.phys.Vec3;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.Redirect;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(Entity.class)
public abstract class EntityMixin {
    @Inject(method="move", at=@At("HEAD"), require=1)
    private void movementobserver$beginMove(MoverType type, Vec3 requested, CallbackInfo ci) {
        MovementObserver.beginMove((Entity)(Object)this, type, requested);
    }

    @Redirect(method="move", at=@At(value="INVOKE", target="Lnet/minecraft/world/entity/Entity;collide(Lnet/minecraft/world/phys/Vec3;)Lnet/minecraft/world/phys/Vec3;"), require=1)
    private Vec3 movementobserver$collide(Entity entity, Vec3 requested) {
        Vec3 clipped = ((EntityInvoker)(Object)this).movementobserver$collide(requested);
        MovementObserver.collided(entity, requested, clipped);
        return clipped;
    }

    @Inject(method="move", at=@At("RETURN"), require=1)
    private void movementobserver$endMove(MoverType type, Vec3 requested, CallbackInfo ci) {
        MovementObserver.endMove((Entity)(Object)this);
    }
}
