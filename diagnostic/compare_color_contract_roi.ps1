[CmdletBinding()]
param(
  [Parameter(Mandatory=$true)][string]$RunRoot,
  [string]$OutputPath = ''
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Add-Type -AssemblyName System.Drawing
if (-not $OutputPath) { $OutputPath = Join-Path $RunRoot 'color-contract-roi.json' }
$java = Join-Path $RunRoot 'java/results'
$rust = Join-Path $RunRoot 'rust/results'
$caseDoc = Get-Content -Raw (Join-Path (Split-Path $PSScriptRoot -Parent) 'diagnostic/color-contract-cases.json') | ConvertFrom-Json
function N($v) { [double]$v }
function V($x,$y,$z) { [pscustomobject]@{x=$x;y=$y;z=$z} }
function Dot($a,$b) { $a.x*$b.x + $a.y*$b.y + $a.z*$b.z }
function Cross($a,$b) { V ($a.y*$b.z-$a.z*$b.y) ($a.z*$b.x-$a.x*$b.z) ($a.x*$b.y-$a.y*$b.x) }
function Camera($m) {
  $yaw=(N $m.cameraYaw)*[math]::PI/180; $pitch=(N $m.cameraPitch)*[math]::PI/180
  $forward=V (-[math]::Sin($yaw)*[math]::Cos($pitch)) (-[math]::Sin($pitch)) ([math]::Cos($yaw)*[math]::Cos($pitch))
  $right=V (-[math]::Cos($yaw)) 0 (-[math]::Sin($yaw)); $up=Cross $right $forward
  $focal=(N $m.height)/(2*[math]::Tan((N $m.fov)*[math]::PI/360))
  [pscustomobject]@{position=(V (N $m.cameraX) (N $m.cameraY) (N $m.cameraZ));forward=$forward;right=$right;up=$up;focal=$focal;width=[int]$m.width;height=[int]$m.height}
}
function Project($c,$x,$y,$z) {
  $d=V ($x-$c.position.x) ($y-$c.position.y) ($z-$c.position.z); $depth=Dot $d $c.forward
  if ($depth -le 0) { throw "target point behind camera: $x,$y,$z" }
  [pscustomobject]@{x=$c.width/2+(Dot $d $c.right)*$c.focal/$depth;y=$c.height/2-(Dot $d $c.up)*$c.focal/$depth}
}
function Crop($c,$blockX,$blockZ,$w,$h) {
  $p=@(); foreach($xx in @($blockX,($blockX + 1))){foreach($yy in @(70,71)){foreach($zz in @($blockZ,($blockZ + 1))){$p += Project $c $xx $yy $zz}}
  }
  [pscustomobject]@{left=[math]::Max(0,[math]::Floor(($p|Measure-Object x -Minimum).Minimum-0.25));top=[math]::Max(0,[math]::Floor(($p|Measure-Object y -Minimum).Minimum-0.25));right=[math]::Min($w,[math]::Ceiling(($p|Measure-Object x -Maximum).Maximum+0.25));bottom=[math]::Min($h,[math]::Ceiling(($p|Measure-Object y -Maximum).Maximum+0.25))}
}
function Pixels($path) {
  $src=[Drawing.Bitmap]::new($path); $width=$src.Width; $height=$src.Height; try {$rect=[Drawing.Rectangle]::new(0,0,$width,$height);$bmp=$src.Clone($rect,[Drawing.Imaging.PixelFormat]::Format32bppArgb)} finally {$src.Dispose()}
  try {$data=$bmp.LockBits([Drawing.Rectangle]::new(0,0,$width,$height),[Drawing.Imaging.ImageLockMode]::ReadOnly,[Drawing.Imaging.PixelFormat]::Format32bppArgb);try{$stride=[math]::Abs($data.Stride);$bytes=[byte[]]::new($stride*$height);[Runtime.InteropServices.Marshal]::Copy($data.Scan0,$bytes,0,$bytes.Length)}finally{$bmp.UnlockBits($data)}}finally{$bmp.Dispose()}
  [pscustomobject]@{width=$width;height=$height;stride=$stride;bytes=$bytes}
}
function RawRoi($a,$b,$crop) {
  $sum=[int64]0;$max=0;$over=0;$pixels=($crop.right-$crop.left)*($crop.bottom-$crop.top)
  for($y=$crop.top;$y -lt $crop.bottom;$y++){for($x=$crop.left;$x -lt $crop.right;$x++){$ia=$y*$a.stride+$x*4;$ib=$y*$b.stride+$x*4;$pm=0;foreach($ch in 0..2){$d=[math]::Abs([int]$a.bytes[$ia+$ch]-[int]$b.bytes[$ib+$ch]);$sum+=$d;if($d -gt $max){$max=$d};if($d -gt $pm){$pm=$d}};if($pm -gt 8){$over++}}}
  [ordered]@{pixels=$pixels;rgbMae=[math]::Round($sum/($pixels*3.0),6);maxDiff=$max;thresholdPixels=$over;thresholdFraction=[math]::Round($over/$pixels,8);metric='raw projected unit-block ROI; background included; no depth mask, alpha mask, rounding or color correction'}
}
$targets=@();$seen=@{}
foreach($case in $caseDoc){$id=[string]$case.caseId;$jdebug=Get-Content -Raw (Join-Path $java "$id.render-debug.json")|ConvertFrom-Json;foreach($t in $jdebug.debugGridTargets){$state=[string]$t.registryState.stateKey;if($state -match '^minecraft:pumpkin_stem\[(age=(0|3|7))\]$'){$key="$($t.x),$($t.y),$($t.z)";if(-not $seen.ContainsKey($key)){$seen[$key]=$true;$targets += [pscustomobject]@{x=[int]$t.x;y=[int]$t.y;z=[int]$t.z;state=$state;selection=[string]$t.selection}}}};foreach($t in $jdebug.samples){$state=[string]$t.stateKey;if($state -match '^minecraft:attached_pumpkin_stem\[facing=west\]$'){$key="$($t.x),$($t.y),$($t.z)";if(-not $seen.ContainsKey($key)){$seen[$key]=$true;$targets += [pscustomobject]@{x=[int]$t.x;y=[int]$t.y;z=[int]$t.z;state=$state;selection='sample-attached-facing-west'}}}}
}
$rows=@();$caseSummaries=@()
foreach($case in $caseDoc){$id=[string]$case.caseId;$jm=Get-Content -Raw (Join-Path $java "$id.json")|ConvertFrom-Json;$rm=Get-Content -Raw (Join-Path $rust "$id.json")|ConvertFrom-Json;$jp=Pixels (Join-Path $java "$id.png");$rp=Pixels (Join-Path $rust "$id.png");$c=Camera $jm;$caseRows=@();foreach($t in $targets){$targetX=[double]$t.PSObject.Properties['x'].Value;$targetZ=[double]$t.PSObject.Properties['z'].Value;if($targetZ -lt 100){throw "bad target state=$($t.state) x=$targetX z=$targetZ raw=$($t.PSObject.Properties['z'].Value)"};$crop=Crop $c $targetX $targetZ ([int]$jp.width) ([int]$jp.height);$raw=RawRoi $jp $rp $crop;$row=[ordered]@{caseId=$id;state=$t.state;selection=$t.selection;x=$t.x;y=$t.y;z=$t.z;roi=$crop;rawRoi=$raw;javaActualSnapshotAt=$jm.actualSnapshotAt;rustActualSnapshotAt=$rm.actualSnapshotAt;clockJava=$jm.clock;clockRust=$rm.clock};$rows+=$row;$caseRows+=$row};$caseSummaries += [ordered]@{caseId=$id;targetCount=$caseRows.Count;meanRgbMae=[math]::Round((($caseRows|ForEach-Object {[double]$_.rawRoi.rgbMae}|Measure-Object -Average).Average),6);rows=$caseRows}}
$result=[ordered]@{schema=1;runRoot=(Resolve-Path $RunRoot).Path;scope='pumpkin stem age0/3/7 and attached_pumpkin_stem; Java DebugLevelSource positions; raw projected unit-block ROIs';foregroundMetric='not added: no common depth mask; rawROI retained';caseSummaries=$caseSummaries;rows=$rows};$result|ConvertTo-Json -Depth 20|Set-Content -LiteralPath $OutputPath -Encoding utf8;Write-Output "color-contract ROI: cases=$($caseDoc.Count) targets=$($targets.Count) report=$OutputPath"