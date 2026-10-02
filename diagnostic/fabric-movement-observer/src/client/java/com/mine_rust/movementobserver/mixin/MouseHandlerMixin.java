package com.mine_rust.movementobserver.mixin;

import com.mine_rust.movementobserver.MovementObserver;
import net.minecraft.client.MouseHandler;
import net.minecraft.client.input.MouseButtonInfo;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(MouseHandler.class)
public abstract class MouseHandlerMixin {
    @Inject(method = "onButton", at = @At("HEAD"))
    private void movementobserver$button(long window, MouseButtonInfo button, int action, CallbackInfo ci) {
        MovementObserver.input("button", window, action, button.button(), -1, button.modifiers(), 0, 0);
    }
    @Inject(method = "onScroll", at = @At("HEAD"))
    private void movementobserver$scroll(long window, double x, double y, CallbackInfo ci) {
        MovementObserver.input("scroll", window, 0, 0, -1, 0, x, y);
    }
    @Inject(method = "onMove", at = @At("HEAD"))
    private void movementobserver$cursor(long window, double x, double y, CallbackInfo ci) {
        MovementObserver.input("cursor", window, 0, 0, -1, 0, x, y);
    }
}
