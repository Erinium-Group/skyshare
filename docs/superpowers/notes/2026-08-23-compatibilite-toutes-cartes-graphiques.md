# Note technique — Couleur pleine résolution sur toutes les cartes graphiques

> Statut : **note de référence, pas un spec.** Aucune décision n'est figée ici.
> Date : 23 août 2026 · Origine : question du propriétaire en fin de jalon 0

---

## 1. Le problème

Le jalon 0 a mesuré que le **4:4:4** — la couleur à pleine résolution — apporte
**+40,1 dB sur le plan U et +29,3 dB sur V** par rapport au 4:2:0, à débit égal.
C'est la différenciation la plus visible du produit face à Discord : c'est ce qui
rend un texte fin lisible au lieu de baveux.

Or cette capacité n'existe pas partout :

| Fabricant | Encodage 4:4:4 | Source |
|-----------|----------------|--------|
| **NVIDIA** | ✅ H.264 et HEVC, **mesuré** sur RTX 4060 | Jalon 0, tâches 1 à 4 |
| **AMD** | ❌ Aucune surface 4:4:4 dans AMF. `AMF_INVALID_FORMAT` sur RDNA 3 | Documentation AMD |
| **Intel** | ❌ Non attesté. 4:2:2 documenté sur certaines générations | Documentation Intel |
| **Mobile** | ❌ Encodage et **décodage** quasi inexistants | Recherche documentaire |

**Le point le plus important, découvert en creusant :** le **décodage** 4:4:4 est
aussi rare que l'encodage. Même en produisant du 4:4:4 depuis une carte NVIDIA, un
spectateur sur téléphone ou sur navigateur ne pourrait pas le décoder. Le problème
n'est donc pas seulement côté émetteur.

**Asymétrie favorable notée :** le décodage 4:4:4 semble fonctionner sur Intel via
Vulkan Video, contrairement à l'encodage. À vérifier, mais cela signifierait qu'un
spectateur Intel peut recevoir du 4:4:4 natif sans contournement.

---

## 2. Décisions de périmètre déjà prises par le propriétaire

1. Le **4:4:4 natif est conservé sur NVIDIA**. Aucune régression pour le matériel
   qui en est capable.
2. Sur les autres cartes — Intel et AMD, dédiées comme intégrées — l'objectif est de
   **faire mieux que le 4:2:0**, pas de s'y résigner.
3. Priorité : **d'abord toutes les cartes sur ordinateur** (Windows, Linux, macOS),
   le mobile ensuite.
4. Sur ordinateur, c'est **l'application native** qui est utilisée, sans exception.
   L'application web est destinée **aux téléphones uniquement**, et sera un pur client
   de visionnage — elle ne capture jamais d'écran.

---

## 3. Les cinq voies, et leur coût

| Voie | Principe | Coût | AMD / Intel | Mobile | Sessions d'encodage |
|------|----------|------|-------------|--------|---------------------|
| **A — Empaquetage** | Luminance et couleur pleine résolution rangées dans **une seule image plus grande**, encodée en 4:2:0 ordinaire | ~2× les pixels | ✅ | ✅ | **1** |
| **B — Flux auxiliaire** | Deux flux 4:2:0 séparés, recombinés à l'affichage | 2 sessions, +30-50 % débit | ✅ | ✅ | 2 |
| **C — Double résolution** | Agrandir ×2 avant d'encoder : la couleur sous-échantillonnée retrouve la résolution native | **4× les pixels** | ✅ | ✅ | 1 |
| **D — Logiciel** | `x264` en 4:4:4 sur le processeur | Charge CPU massive | ✅ | ❌ décodage | 0 (matériel) |
| **E — 4:2:2** | Compromis, double la couleur horizontalement | Modéré | ⚠️ Intel peut-être, AMD non | ⚠️ | 1 |

### Pourquoi la voie A est la candidate à instruire en premier

- **Une seule session d'encodage.** Le budget matériel reste inchangé — déterminant,
  puisque les cartes grand public plafonnent vers huit sessions et que le spec §6.5
  prévoit trois couches de qualité simultanées.
- **Un seul flux réseau**, donc aucune synchronisation à maintenir entre deux flux
  susceptibles de dériver l'un par rapport à l'autre.
- **Aucune exigence de décodage particulière** : le spectateur voit une image 4:2:0
  banale. La recomposition a lieu après décodage, sur son processeur graphique.
- **Aucune perte d'information** : c'est un réarrangement, pas une approximation.

### Pourquoi la voie D est la moins bonne, contrairement à ce qu'on pourrait croire

Le repli logiciel semble la solution évidente — il produit du vrai 4:4:4. Mais il ne
résout **que la moitié du problème** : le spectateur doit encore pouvoir décoder du
4:4:4, ce qui exclut le mobile et les navigateurs. Il coûte cher et ne débloque pas
la cible qui compte.

---

## 4. Ce qu'il faut mesurer avant d'écrire un spec

Aucune de ces valeurs n'est connue. Ce sont les inconnues qui justifient un spike
dédié plutôt qu'une décision sur plan.

1. **Surcoût réel en débit** de la voie A, à qualité perçue égale. L'intuition dit
   « environ le double de pixels donc plus de débit », mais la zone portant la couleur
   est très plate et pourrait compresser bien mieux que l'image principale.
2. **Qualité après recomposition** : mesurer le rapport signal/bruit par plan entre
   l'original et le résultat recomposé, exactement comme au jalon 0 tâche 4. On doit
   retrouver un écart comparable aux +40,1 dB mesurés, sinon le procédé ne tient pas
   sa promesse.
3. **Coût de la recomposition** sur le processeur graphique du spectateur — y compris
   sur une puce intégrée modeste et sur un téléphone.
4. **Comportement à haute résolution** : une image 1440p empaquetée devient plus
   grande que 4K. Les encodeurs AMD et Intel tiennent-ils la cadence à cette taille ?
   C'est la question qui peut invalider la voie A.
5. **Latence ajoutée** par l'empaquetage et la recomposition.
6. **Confirmation matérielle** : aucune carte AMD ni Intel n'était disponible pendant
   le jalon 0. Les constats de la section 1 reposent sur de la documentation, pas sur
   une mesure. **À vérifier sur matériel réel avant tout engagement.**

---

## 5. Conséquences sur l'architecture existante

- **`sky-encode` n'a aucune abstraction.** Il ne contient que l'implémentation NVENC
  en dur, sans trait `VideoEncoder`. Le premier travail sera d'extraire cette
  abstraction — le jalon 0 a déjà montré l'intérêt de l'exercice en séparant la
  plomberie FFI de la logique de sélection.
- **Le spec §6.4 est factuellement faux** sur deux points, déjà relevés au rapport du
  jalon 0 : AV1 ne fait pas de 4:4:4, et H.264 4:4:4 ne peut pas être un « socle
  universel » puisqu'il ne respecte pas la cible de débit sur le matériel testé.
- **Cible de compilation unique** : `x86_64-pc-windows-msvc`. ARM64 n'a jamais été
  abordé.

---

## 6. Questions de produit à trancher, pas d'ingénierie

Elles appartiennent au propriétaire et devront être posées lors du brainstorming du
spec :

1. **Si l'empaquetage coûte trop cher à haute résolution**, que fait-on ? Limiter la
   résolution sur AMD et Intel, ou basculer en 4:2:0 en le disant dans l'interface ?
2. **Faut-il un indicateur visible** dans l'application montrant le mode de couleur
   réellement obtenu ? Un utilisateur qui voit du texte moins net doit pouvoir
   comprendre pourquoi, plutôt que de conclure à un défaut du logiciel.
3. **Le 4:2:2 mérite-t-il d'exister comme palier intermédiaire** si Intel le supporte,
   ou vaut-il mieux n'avoir que deux modes pour limiter les combinaisons à tester ?

---

## 7. Ce que je recommande comme suite

Cette fonctionnalité mérite **son propre spike**, sur le modèle du jalon 0 : un binaire
jetable qui répond aux six inconnues de la section 4 par la mesure, avant qu'un spec
ne fige quoi que ce soit.

Il devrait s'insérer **après le jalon 3** (qualité et couches multiples), qui aura
établi le fonctionnement du simulcast sur NVIDIA — et **avant le jalon 7** (portage
Linux et macOS), qui héritera de l'abstraction d'encodeur produite ici.

Prérequis matériel : **une carte AMD et une carte Intel accessibles pour tester.**
Sans elles, ce spike ne peut pas produire de réponse fiable, exactement comme le
jalon 0 ne peut pas répondre à sa question réseau sans un second réseau.
