[CmdletBinding()]
param(
  [string]$PropertiesPath = 'fabric-render-probe/runtime/fixtures/fluid/server.properties'
)
$ErrorActionPreference = 'Stop'
$marker = 'fabric-render-probe/runtime/fixtures/fluid/tint-fixture-marker.json'
if (-not (Test-Path -LiteralPath $marker)) { throw 'Refusing to target a server without the owned fixture marker.' }
$fixture = Get-Content -LiteralPath $marker -Raw | ConvertFrom-Json
if ($fixture.serverPort -ne 25665 -or $fixture.rconPort -ne 25675 -or $fixture.bounds.minX -ne -3 -or $fixture.bounds.maxX -ne 3) {
  throw 'Owned fixture marker does not match ports 25665/25675 and bounded x/z=-3..3.'
}
$rcon = 'minecraft-debug-server/rcon.ps1'
function Invoke-Rcon([string]$text) {
  $output = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $rcon -Command execute -Text $text -PropertiesPath $PropertiesPath -ServerPort 25675
  if ($LASTEXITCODE -ne 0) { throw "RCON command failed ($LASTEXITCODE): $text`n$($output -join "`n")" }
  $output -join "`n"
}
# Freeze first; cleanup is bounded to this diagnostic's unique tag.
Invoke-Rcon 'tick freeze' | Out-Null
Invoke-Rcon 'time set 6000' | Out-Null
Invoke-Rcon 'kill @e[type=minecraft:item,tag=mine_rust_drop_stone]' | Out-Null
$targetUuid = '00000000-0000-4000-8000-000000000001'
$result = Invoke-Rcon "summon minecraft:item 0.5 64 3 {UUID:[I;0,16384,-2147483648,1],Item:{id:'minecraft:stone',count:1},NoGravity:1b,Motion:[0.0d,0.0d,0.0d],PickupDelay:32767s,Age:0s,Tags:['mine_rust_drop_stone']}"
if ($result -notmatch 'Summoned new Stone') { throw "Drop summon was not confirmed: $result" }
$verify = Invoke-Rcon 'data get entity @e[type=minecraft:item,tag=mine_rust_drop_stone,limit=1]'
if ($verify -notmatch 'minecraft:stone' -or $verify -notmatch 'count: 1') { throw "Summoned drop metadata did not verify: $verify" }
if ($verify -notmatch 'UUID:\s*\[I;\s*0,\s*16384,\s*-2147483648,\s*1\]') { throw "Summoned UUID does not match controlled target: $verify" }
[pscustomobject]@{ summon = $result; targetEntityUUID = $targetUuid; nbt = $verify; sourceNbtFields = 'UUID,Item,NoGravity,Motion,PickupDelay,Age,Tags'; fixtureMarker = [IO.Path]::GetFullPath($marker) } | ConvertTo-Json -Depth 6
