package com.mine_rust.movementobserver.mixin;

import com.mine_rust.movementobserver.MovementObserver;
import net.minecraft.world.entity.player.Player;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.ModifyConstant;

@Mixin(Player.class)
public abstract class PlayerMixin {
    @ModifyConstant(method = "travel", constant = @org.spongepowered.asm.mixin.injection.Constant(doubleValue = 0.6), require = 1)
    private double movementobserver$creativeVerticalDrag(double original) {
        MovementObserver.creativeVerticalDrag((Player)(Object)this, original);
        return original;
    }
}
