package com.mine_rust.movementobserver.mixin;

import com.mine_rust.movementobserver.MovementObserver;
import net.minecraft.client.player.LocalPlayer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(LocalPlayer.class)
public abstract class LocalPlayerMixin {
    @Inject(method = "tick", at = @At("HEAD"))
    private void movementobserver$beforeTick(CallbackInfo ci) { MovementObserver.tick((LocalPlayer)(Object)this, "before_tick"); }
    @Inject(method = "tick", at = @At("TAIL"))
    private void movementobserver$afterTick(CallbackInfo ci) { MovementObserver.tick((LocalPlayer)(Object)this, "after_tick"); }
}
