# Note technique — Couleur pleine résolution sur toutes les cartes graphiques

> Statut : **note de référence.** La décision D1 a été **tranchée le 23/08/2026** par le
> propriétaire — voir §2 point 5. Le reste demeure une matière première : les neuf
> inconnues de la §4 doivent être mesurées avant qu'un spec ne soit écrit.
> Date : 23 août 2026 · Origine : question du propriétaire en fin de jalon 0

---

## 1. Le problème

Le jalon 0 a mesuré que le **4:4:4** — la couleur à pleine résolution — apporte
**+40,1 dB sur le plan U et +29,3 dB sur V** par rapport au 4:2:0, à débit égal.
C'est la différenciation la plus visible du produit face à Discord : c'est ce qui
rend un texte fin lisible au lieu de baveux.

Or cette capacité n'existe pas partout :

### 1.1 Il faut raisonner en deux capacités distinctes, pas une

**Encoder et décoder sont deux circuits séparés.** Une carte peut savoir produire un
format qu'elle ne sait pas lire. C'est le piège central de ce dossier, et le jalon 0
ne l'a pas vu : il écrit le flux dans un fichier relu par un lecteur externe, donc
**il n'a jamais testé le décodage.**

### 1.2 NVIDIA — le seuil est septembre 2018, pas « NVIDIA »

| Génération | Encoder H.264 4:4:4 | Encoder HEVC 4:4:4 | **Décoder** HEVC 4:4:4 |
|------------|:---:|:---:|:---:|
| Maxwell — GTX 900 | ✅ | ❌ | ❌ |
| Pascal — GTX 10xx | ✅ | ❌ | ❌ |
| Volta | ✅ | ❌ | ❌ |
| **Turing — RTX 20xx** | ✅ | ✅ | ✅ |
| Ampere — RTX 30xx | ✅ | ✅ | ✅ |
| Ada — RTX 40xx *(machine de référence)* | ✅ | ✅ | ✅ |
| Blackwell — RTX 50xx | ✅ | ✅ | ✅ |

Deux conséquences que le jalon 0 n'avait pas établies :

1. **Le 4:4:4 exploitable commence à Turing.** Une GTX 1080, haut de gamme récent,
   ne fait pas de HEVC 4:4:4 du tout. Le parc NVIDIA d'avant 2018 est dans la même
   situation qu'AMD.
2. **H.264 4:4:4 n'est décodable en matériel sur aucune carte NVIDIA**, quelle que
   soit sa génération. NVIDIA sait l'encoder depuis dix ans sans jamais avoir su le
   lire. C'est un troisième argument contre ce codec, qui s'ajoute à son débit
   incontrôlable (70 Mbps mesurés au lieu de 10) et à son temps d'encodage double.

### 1.3 Les autres

| Fabricant | Encodage 4:4:4 | Décodage 4:4:4 | Source |
|-----------|----------------|----------------|--------|
| **AMD** | ❌ Aucune surface 4:4:4 dans AMF. `AMF_INVALID_FORMAT` sur RDNA 3 | ❌ non attesté | Documentation AMD |
| **Intel** | ❌ Non attesté. 4:2:2 documenté sur certaines générations | ⚠️ Semblerait fonctionner via Vulkan Video — **à vérifier** | Documentation Intel, Vulkan |
| **Mobile** | ❌ | ❌ | Recherche documentaire |

### 1.4 Reformulation du problème

Ce n'est donc pas « NVIDIA contre les autres ». C'est :

> **NVIDIA de 2018 ou plus récent, contre tout le reste.**

Ce qui élargit considérablement la portée du contournement : la voie retenue ne
servira pas seulement AMD et Intel, elle rattrapera aussi tout le parc NVIDIA
antérieur à Turing, les portables anciens et les téléphones. Un seul mécanisme pour
une couverture quasi totale.

**Asymétrie favorable à confirmer :** le décodage 4:4:4 semble fonctionner sur Intel
via Vulkan Video, contrairement à l'encodage. Si c'est exact, un spectateur Intel
pourrait recevoir du 4:4:4 natif sans contournement — le problème serait alors
strictement côté émetteur pour cette marque.

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
5. **D1 tranchée le 23/08/2026 : la voie A est retenue.** L'empaquetage de la couleur
   pleine résolution dans une image porteuse 4:2:0, recomposée sur le processeur
   graphique du spectateur. Les trois exigences posées sont **cumulatives** —
   compatibilité de toutes les cartes, qualité, fluidité — et aucune ne cède au profit
   des autres. Le 4:4:4 natif reste employé là où le matériel le permet ; l'empaquetage
   prend le relais ailleurs, sans que le produit annonce deux niveaux de promesse.
   Les voies B à E restent documentées comme replis, non comme options ouvertes.
   **Cette décision ne dispense d'aucune des neuf mesures de la §4** : elle fixe la
   direction, pas la faisabilité. Les deux inconnues qui peuvent encore l'invalider sont
   le comportement à haute résolution et les trois pièges de la §3.

---

## 3. Les cinq voies, et leur coût

| Voie | Principe | Coût | AMD / Intel | Mobile | Sessions d'encodage |
|------|----------|------|-------------|--------|---------------------|
| **A — Empaquetage** | Luminance et couleur pleine résolution rangées dans **une seule image plus grande**, encodée en 4:2:0 ordinaire | ~2× les pixels | ✅ | ✅ | **1** |
| **B — Flux auxiliaire** | Deux flux 4:2:0 séparés, recombinés à l'affichage | 2 sessions, +30-50 % débit | ✅ | ✅ | 2 |
| **C — Double résolution** | Agrandir ×2 avant d'encoder : la couleur sous-échantillonnée retrouve la résolution native | **4× les pixels** | ✅ | ✅ | 1 |
| **D — Logiciel** | `x264` en 4:4:4 sur le processeur | Charge CPU **non mesurée** (voir note ci-dessous) | ✅ | ❌ décodage | 0 (matériel) |
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

### La bonne façon de formuler l'idée

Formulation reprise d'un avis extérieur sollicite par le proprietaire, plus claire que
la mienne :

> On transforme la question « ce GPU sait-il encoder du 4:4:4 ? » en
> « ce GPU sait-il encoder une video ordinaire et executer un shader ? »

La seconde question a une reponse positive sur pratiquement tout le materiel des
quinze dernieres annees. Le moteur video ne sert plus qu'a compresser ; c'est le
compute shader qui porte la representation.

### Trois pieges de la voie A, a traiter avant toute mesure

Ils ne sont evoques nulle part ailleurs et conditionnent la faisabilite reelle.

**a) On ne peut pas simplement empiler Y, U et V.** L'image porteuse est elle-meme en
4:2:0 : ses propres plans de couleur sont a quart de resolution. Des donnees rangees
la seraient sous-echantillonnees — on detruirait exactement ce qu'on cherche a
preserver. Le calcul tombe juste, mais seulement avec un rangement qui respecte la
nature de chaque plan :

```
Source 4:4:4 en W x H     -> 3 x W x H echantillons
Porteuse 4:2:0 en W x 2H  -> Y = 2WH, U = V = WH/2  -> total 3WH
```

L'essentiel doit aller dans le plan de luminance, seul plan a pleine resolution.
Ce n'est pas un empilement, c'est un decoupage reflechi.

**b) Les frontieres entre zones creeront des artefacts.** L'encodeur travaille par
blocs et predit chaque bloc a partir de ses voisins. Aux jointures entre la zone de
luminance et les zones de couleur, il rencontrera des discontinuites brutales qui
n'existent dans aucune image naturelle, et y repondra par des artefacts de blocs —
lesquels reapparaitront apres recomposition sous forme de bandes de couleur fausse.
Il faudra des marges tampons, ou un arrangement preservant la continuite spatiale.
C'est le vrai travail delicat de cette technique.

**c) Le modele perceptuel de l'encodeur joue contre nous.** Un encodeur repartit son
debit selon ce qu'il estime important pour l'oeil. Il croira regarder une image et
ignorera que la moitie basse porte des donnees de couleur : il pourrait donc
sacrifier la precision chromatique en pensant compresser une zone peu detaillee —
l'inverse exact du but recherche. C'est ce qui rend la mesure de qualite apres
recomposition (inconnue n2 de la section 4) non negociable avant tout engagement.

> **Aucun chiffre n'est disponible sur le coût de la voie D.** Le jalon 0 n'a jamais
> mesuré d'encodage logiciel, et toute valeur avancee ici — y compris dans mes propres
> messages anterieurs — serait une estimation. Elle doit etre mesuree avant d'ecarter
> ou de retenir cette voie, d'autant qu'elle pese sur la decision D1, reservee au
> proprietaire.

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
7. **Capacités de décodage**, jamais testées au jalon 0 — le spike écrit dans un
   fichier relu par un lecteur externe. À mesurer par génération et par fabricant,
   séparément de l'encodage.
8. **Coût d'un transfert entre deux cartes**, par image, dans les deux topologies :
   portable hybride sur lien PCIe interne, et e-GPU sur Thunderbolt. Les debits
   theoriques du §5.1 doivent etre confrontes a la mesure — latence par image,
   variabilite, et comportement quand le lien approche la saturation. C'est ce qui
   determinera si le pari zero-copie du jalon 0 tient hors d'une configuration a
   carte unique, et a quelle resolution il cesse de tenir.
9. **Coût du décodage logiciel** en dernier recours, pour un spectateur dont aucune
   carte ne sait lire le format reçu — et son plafond en nombre de flux simultanés,
   le spec §7.1 promettant six flux en 1440p60 pour environ 15 % d'un processeur
   graphique moderne, chiffre qui suppose un décodage matériel.

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

### 5.1 La détection matérielle doit être refondue — demande du propriétaire

`sky-probe hw` existe déjà et interroge NVENC pour connaître les codecs encodables.
Il est insuffisant sur trois plans.

**a) Il ne détecte que l'encodage.** Or, comme établi en §1.1, encoder et décoder
sont deux circuits distincts. Une carte peut produire un format qu'elle ne sait pas
lire — c'est précisément le cas de H.264 4:4:4 sur toutes les cartes NVIDIA. La
détection doit interroger **les deux** capacités séparément.

**b) Il suppose une seule carte.** Trois configurations très répandues le mettent en
défaut :

| Configuration | Difficulté |
|---------------|-----------|
| **Portable hybride** — puce Intel intégrée + carte NVIDIA dédiée | L'écran est piloté par l'une, l'encodeur performant est sur l'autre |
| **Carte externe (e-GPU)** | Ajoute un lien Thunderbolt entre les deux, avec sa latence propre |
| **Bureau à plusieurs cartes** | Quelle carte capture, quelle carte encode ? |

**Un e-GPU n'est pas une marque de plus, c'est une topologie de plus.** La carte qu'il
contient reste une NVIDIA, AMD ou Intel ordinaire, avec les capacités decrites en §1.
Ce qui change est le lien entre elle et le reste de la machine — et ce lien se chiffre.

Debit necessaire pour transferer chaque image **non compressee** (RGBA 8 bits) :

| Cas | Debit | Part d'un Thunderbolt 3/4 (~22 Gb/s utiles) |
|-----|-------|---------------------------------------------|
| 1440p a 60 im/s | ~7 Gb/s | 32 % — confortable |
| 4K a 60 im/s | ~16 Gb/s | 73 % — serre |
| 1440p a 144 im/s | ~17 Gb/s | 77 % — serre |
| 4K a 144 im/s | ~38 Gb/s | **impossible** |

**Ces chiffres ne valent que dans un seul cas de figure**, et c'est ce qui rend la
detection de topologie decisive :

| Ou est branche l'ecran | Consequence |
|------------------------|-------------|
| **Sur l'e-GPU** | Capture et encodage sur la meme carte. Aucun transfert. La chaine sans copie du jalon 0 s'applique telle quelle. |
| **Sur le portable** (ecran interne ou sortie integree) | La texture nait sur la puce integree, l'encodeur vise est sur l'e-GPU. Transfert Thunderbolt a chaque image, avec les debits ci-dessus. |

La meme distinction vaut pour un portable hybride sans e-GPU, avec un lien PCIe interne
plus rapide (typiquement PCIe 4.0 x8 ou x16, largement au-dessus des besoins) — ce qui
rend ce cas nettement moins critique que celui de l'e-GPU.

**Le problème de fond que cela révèle :** le jalon 0 a validé une chaîne **sans
aucune copie vers la mémoire centrale**, et c'est ce qui donne les 0,53 % de
processeur mesurés. Mais cette validation portait sur **une seule carte**. Sur un
portable hybride, la texture capturée vit sur la puce qui pilote l'écran tandis que
l'encodeur visé est sur l'autre carte : une copie devient inévitable, et le pari
zéro-copie tombe partiellement.

C'est une inconnue de plus à mesurer — le coût réel d'un transfert entre deux cartes
par image, à 60 images par seconde. Elle n'était identifiée nulle part avant que le
propriétaire pose la question.

**c) La détection ne suffit pas : il faut une négociation.** L'émetteur ne peut pas
choisir son format tout seul, puisque le format doit être **décodable par chaque
spectateur**. Le spec §6.4 prévoit déjà « détection matérielle au premier lancement,
puis négociation avec chaque spectateur » — mais la moitié décodage de cette
détection n'existe pas.

Conséquence concrète avec plusieurs spectateurs : si l'un d'eux ne sait pas décoder
le 4:4:4, faut-il dégrader pour tout le monde, ou produire deux formats différents ?
La réponse dépend du mécanisme de couches de qualité du jalon 3, et doit être posée
là.

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
