#!/usr/bin/env bash
# Mesure PSNR/SSIM par plan + extraction d'images pour le comparatif des 4
# codecs (Tache 4). A executer depuis D:/skyshare/spike, apres :
#   cargo run --release -p sky-probe -- codecs --seconds 15 --bitrate-mbps 10
#
# Methode (voir spike/docs/comparatif-codecs.md pour le detail complet) :
# - Reference : sequence BGRA brute (cmp-reference.bgra) recalculee cote CPU,
#   bit a bit identique a ce que chaque encodeur a recu (verifie
#   manuellement sur plusieurs pixels connus).
# - setpts=N/(60*TB) sur les DEUX flux avant comparaison : les horodatages
#   devines par les demuxeurs varient sauvagement d'un codec a l'autre
#   (120fps devine pour H.264, 60fps pour HEVC, 25fps pour AV1 -- aucun des
#   trois flux elementaires ne porte de timing fiable), et le filtre
#   psnr/ssim aligne les paires par horodatage. Sans ce filtre, l'alignement
#   image par image ne serait pas garanti. Avec, l'image k du flux decode
#   est comparee a l'image k de la reference, par construction.
# - trim=start_frame=120:end_frame=901 sur les DEUX flux : les 4 flux codent
#   tous exactement 901 images (verifie), la reference brute en contient 1020
#   (marge de securite) -- d'ou end_frame=901 pour les deux. start_frame=120
#   exclut le piege documente du tampon VBV a 1 image (rampe de qualite sur
#   la premiere image cle, plateau atteint vers l'image 95 -- verifie sur les
#   logs psnr_y). Applique aux DEUX flux (pas seulement la reference) pour
#   que l'image k du flux garde sa correspondance avec l'image k de la
#   reference apres coupe -- trim ne renumerote pas tout seul, c'est setpts
#   juste apres qui repart de 0 sur le premier survivant des deux cotes.
#   Sans le end_frame=901, le filtre "framesync" repete la derniere image du
#   flux le plus court au-dela de son EOF pour continuer a produire des
#   sorties -- contaminant la moyenne avec des paires non correspondantes.
# - Conversion RGB->YUV444p de la reference : scale=out_range=full (matrice
#   par defaut = bt601, coefficients identiques a BT.470BG que NVENC a
#   utilise). Verifie sur un pixel rouge pur (255,0,0) : Y=76 U=84 V=255,
#   conforme a la formule BT.601 pleine plage.
# - Comparaison en yuv444p pour les 4 flux : passer un flux 4:2:0 par
#   -pix_fmt yuv444p ne fait qu'suréchantillonner les plans U/V (YUV->YUV,
#   pas de nouvelle conversion de matrice), donc pas de biais introduit par
#   cette etape pour les flux 4:2:0.

set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

REF=cmp-reference.bgra
W=2560
H=1440
FRAMES=901
START=120   # exclut la rampe VBV -- voir commentaire de methode plus haut

declare -A FICHIERS=(
  [h264-420]=cmp-h264-420.h264
  [h264-444]=cmp-h264-444.h264
  [hevc-444]=cmp-hevc-444.h265
  [av1-420]=cmp-av1-420.ivf
)

mkdir -p mesures
rm -f mesures/*.log mesures/*.png

# Image de reference a l'index 120 (evite les 2 premieres secondes -- piege
# documente du tampon VBV a 1 image sur la premiere image cle).
ffmpeg -v error -y -f rawvideo -pix_fmt bgra -video_size ${W}x${H} -framerate 60 -i "$REF" \
  -vf "select=eq(n\,120)" -vframes 1 mesures/frame120-reference.png
ffmpeg -v error -y -i mesures/frame120-reference.png -vf "crop=340:170:200:94" mesures/crop-reference.png

for nom in "${!FICHIERS[@]}"; do
  f="${FICHIERS[$nom]}"
  echo "=== $nom ($f) ==="

  ffmpeg -v error -y -f rawvideo -pix_fmt bgra -video_size ${W}x${H} -framerate 60 -i "$REF" -i "$f" \
    -lavfi "[0:v]trim=start_frame=${START}:end_frame=${FRAMES},setpts=N/(60*TB),scale=out_range=full,format=yuv444p,split=2[r1][r2];[1:v]trim=start_frame=${START}:end_frame=${FRAMES},setpts=N/(60*TB),format=yuv444p,split=2[t1][t2];[r1][t1]psnr=stats_file=mesures/psnr-${nom}.log;[r2][t2]ssim=stats_file=mesures/ssim-${nom}.log" \
    -f null -

  # Image a l'index 120 du flux decode (complete + recadree sur le panneau
  # de texte), pour comparaison visuelle avec la reference.
  ffmpeg -v error -y -i "$f" -vf "select=eq(n\,120)" -vframes 1 "mesures/frame120-${nom}.png"
  ffmpeg -v error -y -i "mesures/frame120-${nom}.png" -vf "crop=340:170:200:94" "mesures/crop-${nom}.png"

  taille=$(stat -c%s "$f")
  echo "  taille   : ${taille} octets"
done

echo
echo "=== Moyennes PSNR / SSIM par plan (images 120-900, 781 images -- rampe VBV des 2 premieres secondes reellement exclue) ==="
printf "%-10s %8s %8s %8s %8s | %8s %8s %8s %8s\n" "codec" "psnr_y" "psnr_u" "psnr_v" "psnr_avg" "ssim_y" "ssim_u" "ssim_v" "ssim_all"
for nom in h264-420 h264-444 hevc-444 av1-420; do
  awk -v c="$nom" '
    {
      for (i=1;i<=NF;i++) {
        split($i, kv, ":");
        v[kv[1]] += kv[2]; n[kv[1]]++;
      }
    }
    END {
      printf "%-10s %8.2f %8.2f %8.2f %8.2f", c, v["psnr_y"]/n["psnr_y"], v["psnr_u"]/n["psnr_u"], v["psnr_v"]/n["psnr_v"], v["psnr_avg"]/n["psnr_avg"];
    }' "mesures/psnr-${nom}.log"
  awk -v c="$nom" '
    {
      for (i=1;i<=NF;i++) {
        split($i, kv, ":");
        if (kv[1]=="Y"||kv[1]=="U"||kv[1]=="V"||kv[1]=="All") { v[kv[1]] += kv[2]; n[kv[1]]++; }
      }
    }
    END {
      printf " | %8.4f %8.4f %8.4f %8.4f\n", v["Y"]/n["Y"], v["U"]/n["U"], v["V"]/n["V"], v["All"]/n["All"];
    }' "mesures/ssim-${nom}.log"
done
