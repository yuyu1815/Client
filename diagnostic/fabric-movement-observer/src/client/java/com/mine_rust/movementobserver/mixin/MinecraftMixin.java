package com.mine_rust.movementobserver.mixin;

import com.mine_rust.movementobserver.MovementObserver;
import net.minecraft.client.Minecraft;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(Minecraft.class)
public abstract class MinecraftMixin {
    @Inject(method="runTick(Z)V", at=@At("HEAD"), require=1)
    private void movementobserver$frameStart(boolean renderLevel, CallbackInfo ci) { MovementObserver.frameStart(); }
    @Inject(method="runTick(Z)V", at=@At("TAIL"), require=1)
    private void movementobserver$frameEnd(boolean renderLevel, CallbackInfo ci) { MovementObserver.frameEnd((Minecraft)(Object)this); }
}
