# Construit la version portable de SkyShare : un dossier (SkyShare.exe + la fiche d'essai)
# et son archive, à copier sur une machine qui n'a ni Rust, ni Node, ni dossier de développement.
#
# PowerShell 5.1 : pas de && ni de ?:, et chaque commande native est suivie d'un contrôle
# explicite de $LASTEXITCODE (un échec natif ne lève aucune exception en 5.1).
#
# Ce script lance `tauri build`, qui ne doit ouvrir AUCUNE fenêtre. Il n'exécute jamais
# SkyShare.exe.
#
# Usage, depuis n'importe où :  powershell -File D:\skyshare\spike\scripts\version-portable.ps1

$ErrorActionPreference = "Continue"

function Echouer([string]$message) {
    Write-Host ""
    Write-Host "ÉCHEC : $message" -ForegroundColor Red
    exit 1
}

# --- Chemins : la racine du dépôt est deux niveaux au-dessus de spike\scripts ---------------
$racine = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$app = Join-Path $racine "app"
$sky_app = Join-Path $racine "spike\crates\sky-app"
$exe_construit = Join-Path $racine "spike\target\release\sky-app.exe"
$fiche = Join-Path $racine "spike\docs\essai-toutes-cartes.md"
$dist = Join-Path $racine "dist"
$dossier = Join-Path $dist "SkyShare-portable"
$archive = Join-Path $dist "SkyShare-portable.zip"
$tauri = Join-Path $app "node_modules\.bin\tauri.cmd"

if (-not (Test-Path $fiche)) { Echouer "la fiche d'essai est introuvable : $fiche" }
if (-not (Test-Path $tauri)) { Echouer "la CLI Tauri est introuvable ($tauri) : lancer d'abord « npm install » dans app\." }

# --- 1. L'interface ------------------------------------------------------------------------
Write-Host "== 1/6 npm run build (app\)"
Push-Location $app
npm run build
$code = $LASTEXITCODE
Pop-Location
if ($code -ne 0) { Echouer "« npm run build » a rendu le code $code." }

# --- 2. L'application empaquetée -----------------------------------------------------------
# La seule commande de construction qui fonctionne (CLAUDE.md) : depuis sky-app, par la CLI
# installée dans app\. « npx tauri build » depuis spike\ ne construit rien.
Write-Host "== 2/6 tauri build (spike\crates\sky-app)"
Push-Location $sky_app
& $tauri build
$code = $LASTEXITCODE
Pop-Location
if ($code -ne 0) { Echouer "« tauri build » a rendu le code $code." }
if (-not (Test-Path $exe_construit)) { Echouer "l'exécutable attendu est absent : $exe_construit" }

# --- 3. Le dossier portable ----------------------------------------------------------------
Write-Host "== 3/6 dossier portable"
if (Test-Path $dossier) { Remove-Item -Recurse -Force $dossier -ErrorAction Stop }
New-Item -ItemType Directory -Force $dossier -ErrorAction Stop | Out-Null
Copy-Item $exe_construit (Join-Path $dossier "SkyShare.exe") -ErrorAction Stop
Copy-Item $fiche (Join-Path $dossier "essai-toutes-cartes.md") -ErrorAction Stop
$exe = Join-Path $dossier "SkyShare.exe"

# --- 4. Les dépendances de chargement ------------------------------------------------------
# Une DLL NVIDIA importée (nvcuda, nvcuvid, nvEncodeAPI64) empêcherait l'exécutable de
# démarrer sur une machine sans NVIDIA : elles doivent être chargées dynamiquement.
# mfplat.dll, elle, est une dépendance de chargement assumée (spec du sous-jalon 1, §9).
Write-Host "== 4/6 dumpbin /dependents"
$dumpbin = Get-ChildItem "C:\Program Files (x86)\Microsoft Visual Studio" -Recurse -Filter dumpbin.exe -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -like "*Hostx64\x64*" } |
    Select-Object -First 1
if ($null -eq $dumpbin) { Echouer "dumpbin.exe (Hostx64\x64) est introuvable sous C:\Program Files (x86)\Microsoft Visual Studio." }

$sortie = & $dumpbin.FullName /dependents $exe
if ($LASTEXITCODE -ne 0) { Echouer "dumpbin a rendu le code $LASTEXITCODE." }

$dependances = @()
foreach ($ligne in $sortie) {
    if ($ligne -match '^\s+(\S+\.dll)\s*$') { $dependances += $Matches[1] }
}
if ($dependances.Count -eq 0) { Echouer "dumpbin n'a listé aucune dépendance : sortie illisible ?" }

Write-Host "   $($dependances.Count) dépendances relevées :"
foreach ($d in ($dependances | Sort-Object -Unique)) { Write-Host "     $d" }

$nvidia = @($dependances | Where-Object { $_ -like "nv*" })
if ($nvidia.Count -gt 0) {
    Echouer ("l'exécutable importe une DLL NVIDIA (" + ($nvidia -join ", ") + ") : il ne démarrerait pas sur une machine sans NVIDIA.")
}
$mfplat = @($dependances | Where-Object { $_ -ieq "mfplat.dll" })
if ($mfplat.Count -eq 0) { Echouer "mfplat.dll n'est pas parmi les dépendances : l'hypothèse du §9 de la spec est à revoir." }
Write-Host "   aucune DLL « nv* » ; mfplat.dll présente."

# --- 5. L'archive --------------------------------------------------------------------------
Write-Host "== 5/6 archive"
if (Test-Path $archive) { Remove-Item -Force $archive -ErrorAction Stop }
Compress-Archive -Path (Join-Path $dossier "*") -DestinationPath $archive -ErrorAction Stop

# --- 6. Le bilan ---------------------------------------------------------------------------
Write-Host "== 6/6 bilan"
$taille = (Get-Item $archive).Length
$empreinte = (Get-FileHash -Algorithm SHA256 $exe).Hash
Write-Host ("   archive  : {0}" -f $archive)
Write-Host ("   taille   : {0} octets ({1:N1} Mo)" -f $taille, ($taille / 1MB))
Write-Host ("   SHA-256 de SkyShare.exe : {0}" -f $empreinte)
Write-Host ""
Write-Host "Terminé. Pensez à vérifier, avant tout commit :"
Write-Host "  git diff --ignore-cr-at-eol spike/crates/sky-app/Cargo.toml   (doit être vide)"
Write-Host "  git checkout -- spike/crates/sky-app/Cargo.toml"
exit 0
