# Essai du jalon 2 — fiche pour le propriétaire

Ce que l'essai prouve : **un pixel juste apparaît à l'écran** — couleur, netteté, états, plein
écran —, **la dernière image d'un écran qui cesse de bouger s'affiche**, et ce que coûte le décodage
**pendant** un encodage sur la même carte. Il ne prouve rien du réseau : l'écart 7 attend l'essai à
deux machines sur deux réseaux.

Rédigée le 02/10/2026 en relisant le code (`sky-probe/src/main.rs`, `cmd_host.rs`, `cmd_view.rs`,
`cmd_compte.rs`, `sky-partage/src/hote.rs`, `spectateur.rs`), **sans rien exécuter**, puis mise à
jour après la vague de correction finale de la branche. Tout ce qui est écrit « déduit du code »
n'a jamais été observé.

**Toutes les commandes se tapent dans Git Bash**, une par bloc, dans l'ordre. **Chaque commande qui
ouvre une fenêtre ou un navigateur est annoncée juste au-dessus d'elle.**

---

## ⛔ Pendant tout l'essai

- **Ne jamais définir `RUST_LOG="str0m=debug"`** (ni aucun `RUST_LOG` visant `str0m`). Les traces
  les plus bavardes de `str0m` écrivent l'adresse des deux machines **en clair**, donc dans toute
  capture d'écran que tu partagerais ensuite. Le filtrage `pii` ne les couvre pas. (Ce qui protège
  vraiment, c'est qu'aucun collecteur de traces n'est installé ; la consigne reste, par prudence.)
- **Garde l'application SkyShare (celle de l'icône près de l'horloge) fermée pendant tous les
  essais, du §3 au §7.** Elle synchronise toute seule — et peut démarrer avec Windows si l'option
  est cochée — et
  consommerait les messages de la négociation (limite connue depuis le C2). Si elle s'est relancée,
  quitte-la par l'icône → **Quitter** avant de continuer.

---

## 1. Premier geste : l'application démarre-t-elle ?

L'application empaquetée **n'a jamais été lancée avec son nouveau manifeste** (conscience DPI,
tâche 10). Un manifeste invalide empêcherait le démarrage : c'est la première chose à vérifier.

Quitte d'abord le SkyShare installé : icône près de l'horloge → **Quitter**. Sinon l'instance
unique renverrait vers lui au lieu de lancer la nouvelle.

```
cd /d/skyshare/app && npm run build
```

```
cd /d/skyshare/spike/crates/sky-app && ../../../app/node_modules/.bin/tauri build
```

`npx tauri build` depuis `spike/` **ne construit rien** : n'utilise que les deux commandes
ci-dessus.

> **⚠️ La commande suivante OUVRE LA FENÊTRE DE L'APPLICATION** et pose son icône près de l'horloge.

```
/d/skyshare/spike/target/release/sky-app.exe
```

À vérifier : la fenêtre s'ouvre, l'interface est remplie (pas « localhost a refusé de se
connecter »), l'icône apparaît près de l'horloge. **Puis quitte-la par l'icône → Quitter**, et
garde-la fermée jusqu'à la fin des essais (voir ⛔ plus haut).

Après la construction, `git status` montre `spike/crates/sky-app/Cargo.toml` modifié : **c'est
normal**, la construction réécrit ses fins de ligne. `git diff --ignore-cr-at-eol` doit être vide.
Ne pas le commiter.

## 2. Construire `sky-probe` en `--release`

```
cd /d/skyshare/spike && cargo build --release -p sky-probe
```

L'exécutable présent date du 27/09 : il ne contient rien du jalon 2. **Toujours `--release`** pour
ton compte principal : en `debug`, `sky-probe` ouvre un coffre distinct (`SkyShare.dev`), vide ; s'y
connecter avec ton compte principal puis enregistrer un appareil créerait un **second appareil sur
ton compte réel**.

## 3. ⚠️ À trancher avant l'essai : il faut deux identités sur la machine

**Déduit du code, non essayé.** `view` ne peut regarder qu'un **ami** (`sky-compte/src/annuaire.rs:781`
cherche dans `etat.amis`) : ton propre compte n'en est pas un. Et une build `--release` n'a qu'une
identité par machine (`sky-compte/src/coffre.rs:104`). La commande prévue par le plan,
`view <nom-de-l-appareil>`, **ne peut donc pas fonctionner** telle quelle.

La seule seconde identité possible sur cette machine est le coffre de développement. D'où ce
montage, **qui déroge à la lettre de la règle « jamais en `debug` »** et que toi seul peux
accepter :

| Rôle | Build | Compte |
|---|---|---|
| hôte (`host`) | `debug` | ton **second** compte Discord (celui du portable au C2) |
| spectateur (`view`) | `--release` | ton compte principal |

**Pourquoi il respecte l'esprit de la règle.** La règle existe parce qu'un `login` en `debug` sur
ton compte **principal** remplirait le coffre `.dev`, vide, et qu'un `device register` y créerait un
second appareil sur ton compte réel (leçon du 20/09). Ici, le coffre `.dev` est **séparé** du coffre
de la build `--release` (un service par profil, `coffre.rs`) et il reçoit l'identité de ton
**second** compte : **rien n'est créé sur ton compte principal**, et l'identité `--release` de la
machine n'est pas touchée. C'est même l'usage pour lequel le coffre `.dev` existe : porter une
identité distincte.

Le spectateur est en `--release` parce que c'est lui qu'on mesure. Les chiffres de l'**hôte**, en
`debug`, ne sont pas représentatifs.

### Préparation, une seule fois, avec le second compte

**Se connecter au second compte.** Si ton navigateur est déjà connecté à Discord avec ton compte
principal, l'autorisation se ferait sur lui **sans même te demander de compte**. Avant la commande
suivante, ouvre donc une **fenêtre de navigation privée**, connecte-toi à discord.com avec le
**second** compte, et laisse-la ouverte ; ou déconnecte ton compte principal de Discord dans le
navigateur par défaut. (Déduit : la commande ouvre le navigateur **par défaut** ; si elle ouvre un
onglet normal alors que tu es connecté en privé, copie l'adresse de cet onglet dans la fenêtre
privée.)

> **⚠️ La commande suivante OUVRE TON NAVIGATEUR** sur la page d'autorisation Discord.

```
cd /d/skyshare/spike && cargo run -p sky-probe -- login
```

Avant d'autoriser dans le navigateur, **lis le nom du compte affiché par Discord**. Si c'est ton
compte principal, n'autorise pas. À la fin, le terminal affiche « Connecté en tant que … ».

**Si c'est ton compte principal, arrête-toi là, ne lance PAS la commande suivante, et dis-le moi.**
`login` n'a créé qu'une **session** sur ton compte (aucun appareil : le coffre `.dev` n'en a pas,
vérifié dans `cmd_compte.rs`) ; c'est `device register` qui créerait l'appareil de trop.

L'enregistrement qui suit crée une **ligne persistante en production**, sur ton **second** compte :
un appareil nommé « PC-fixe-essai », visible de ses amis, qui reste après l'essai. Il se révoque
depuis « Mon compte » de l'application, connectée à ce second compte.

```
cd /d/skyshare/spike && cargo run -p sky-probe -- device register "PC-fixe-essai"
```

```
cd /d/skyshare/spike && cargo run -p sky-probe -- friends list
```

Ton compte principal doit y figurer comme ami. Note le **nom Discord exact du second compte** : le
spectateur le désignera.

*Autre montage possible pour l'essai visuel seul* : le portable en spectateur, en `--release`, si
sa carte NVIDIA décode le HEVC 4:4:4 (sans cela, il ne peut pas regarder — refus à l'ouverture).
Mais la mesure du §6 exige les deux rôles **sur la même machine**.

## 4. Lancer un essai

Toujours dans cet ordre : l'hôte d'abord, le spectateur ensuite. Pendant l'attente, **aucune autre
commande `sky-probe`**, et l'application de l'icône **fermée** (elles consommeraient les messages de
la négociation).

Hôte, avec l'image de test et non ton écran. **Ne change pas `--format`** : la valeur par défaut,
`auto`, propose tous les formats que ta carte encode, et c'est la négociation avec le
spectateur qui retient le meilleur commun (HEVC 4:4:4 entre deux cartes récentes). Forcer
`--format hevc444` donne le même résultat sur ces cartes. La colonne « RTT » du terminal de l'hôte affiche
**« non mesuré »** : aucune mesure d'aller-retour n'existe côté hôte.

```
cd /d/skyshare/spike && cargo run -p sky-probe -- host --source synthetique --seconds 90
```

> **⚠️ La commande suivante OUVRE UNE FENÊTRE** « SkyShare — écran de … », après la connexion.
> La fermer par la croix arrête le visionnage.

Spectateur (remplace le nom par celui du second compte, guillemets compris) :

```
cd /d/skyshare/spike && ./target/release/sky-probe.exe view "NomDuSecondCompte" --seconds 60
```

Le flux reçu est écrit dans `spike/recu.h265` (exclu par git). Avec la source synthétique, il ne
contient que l'image de test ; supprime-le après l'essai.

## 5. Ce qu'il faut vérifier, à l'œil

L'image de test est un faux éditeur de code : fond sombre, « lignes de texte » dont chaque glyphe
est fait de colonnes d'**un pixel**, alternées **rouge pur / bleu pur**.

- [ ] **Couleur juste.** Fond gris-bleu très sombre, glyphes rouges et bleus, sans dominante. Une
  dominante sur tout l'écran trahirait une matrice fausse **après** le décodeur (le décodeur
  lui-même est couvert par un test). En fenêtre de 1280×720, l'image est réduite de moitié et les
  colonnes se mélangent en violet : ce n'est **pas** un défaut (déduit du code).
- [ ] **Netteté à 150 %.** Règle la mise à l'échelle de Windows à 150 %, relance le spectateur,
  passe en plein écran (F11) puis grossis avec la Loupe de Windows : si ton écran est en
  2560×1440 (celui du jalon 0), l'image y est à l'échelle 1 et les colonnes rouges et bleues d'un
  pixel doivent rester **distinctes**. Un violet uniforme et flou en plein écran signalerait une
  conscience DPI manquante (ou un 4:2:0). La source synthétique n'a pas de vrai texte : les essais D
  et E du §7, sur ton vrai écran, le permettent.
- [ ] **Les trois états sans image** — aucune fenêtre noire muette :
  - « En attente de l'image… » : à l'ouverture de la fenêtre, avant la première image clé. Peut
    être trop bref pour être vu ; « non observé » est une réponse acceptable.
  - « Le partage s'est arrêté » : voir l'essai B ci-dessous.
  - « Connexion perdue » : voir l'essai C ci-dessous.
- [ ] **F11** bascule en plein écran et en revient ; le rapport d'image est conservé (bandes noires
  au besoin). Échap ne sort pas du plein écran : c'est connu.

## 6. Ce qu'il faut relever

**« Non mesuré » est une réponse acceptable. Une estimation présentée comme une mesure ne l'est
pas.** Avant de lancer l'essai, ouvre un troisième terminal :

```
nvidia-smi dmon
```

Ses colonnes `enc` et `dec` donnent l'occupation de l'encodeur et du décodeur. Le spectateur, lui,
imprime une ligne par relevé : `Mbps | images reçues/s | gigue | décodage | images écartées`.

| Grandeur | Où la lire | Remarque |
|---|---|---|
| Images reçues par seconde | ligne du spectateur | ce sont les images **arrivées**, écartées comprises, pas les images affichées. L'hôte vise 60 im/s (constante du code) ; en `debug`, il peut produire moins — déduit, non mesuré |
| Latence de décodage | ligne du spectateur, « décodage » | une **moyenne** par relevé, pas une médiane ; la sonde avait mesuré 1,57 ms médiane, décodage seul |
| Latence capture → pixel | — | **aucun instrument du dépôt ne la mesure**. Le « transit » du bilan compare deux horloges d'origines différentes : ce n'est pas elle |
| Débit | ligne du spectateur, et bilan final | |
| Images écartées | ligne du spectateur | **déduit du code** : sur un essai sans perte, elle devrait rester à 0 ou presque — les images décodées avant la première image clé y sont comptées (`spectateur.rs`, `abandonner`), donc quelques-unes au démarrage ne sont pas un défaut. Un compte qui **monte en continu** en est un |
| Occupation `enc` / `dec` | `nvidia-smi dmon` | à relever **pendant** l'essai |

**Décodage pendant un encodage (60 s).** L'essai A ci-dessous *est* cette mesure : sur la même
machine, l'hôte encode pendant que le spectateur décode. Relève, sur ses 60 secondes : images
reçues/s et décodage côté spectateur, `enc` et `dec` dans `nvidia-smi dmon`, et si l'un des deux
**s'effondre**. Si cela ne tient pas, c'est une limite à écrire, pas un échec de l'essai.

## 7. Les essais

### A, B, C — avec l'image de test

- **A — le principal (60 s, mesures et contrôles visuels).** Commandes du §4 telles quelles : hôte
  90 s, spectateur 60 s (**la commande du spectateur ouvre la fenêtre**). Le spectateur finit sur
  son bilan (« Débit moyen reçu … »).
- **B — l'arrêt annoncé.** Hôte :

  ```
  cd /d/skyshare/spike && cargo run -p sky-probe -- host --source synthetique --seconds 20
  ```

  > **⚠️ La commande suivante OUVRE UNE FENÊTRE** « SkyShare — écran de … ».

  ```
  cd /d/skyshare/spike && ./target/release/sky-probe.exe view "NomDuSecondCompte" --seconds 120
  ```

  Attendu (déduit du code) : la fenêtre affiche « Le partage s'est arrêté », puis le terminal du
  spectateur « Ton ami a arrêté son partage. » — **et pas « ÉCHEC »**. Un « ÉCHEC » ici serait un
  défaut à signaler.
- **C — la connexion perdue.** Hôte :

  ```
  cd /d/skyshare/spike && cargo run -p sky-probe -- host --source synthetique --seconds 300
  ```

  > **⚠️ La commande suivante OUVRE UNE FENÊTRE** « SkyShare — écran de … ».

  ```
  cd /d/skyshare/spike && ./target/release/sky-probe.exe view "NomDuSecondCompte" --seconds 120
  ```

  Une fois l'image affichée, **Ctrl+C dans le terminal de l'hôte** (il s'arrête sans rien
  annoncer). Attendu (déduit) : « Connexion perdue » dans la fenêtre, « ÉCHEC : … » au terminal —
  ici c'est juste. Le délai avant que la perte soit vue n'est **pas mesuré** : le noter.

### D et E — avec ton vrai écran : les seuls capables de révéler deux défauts

La source synthétique produit 60 images par seconde **quoi qu'il arrive** : elle ne peut pas voir
ce qui se passe quand l'écran **cesse de bouger**, alors que la capture Windows ne livre une image
que quand l'écran change. Deux défauts corrigés par la vague finale vivaient exactement là, et
**seuls ces deux essais peuvent confirmer qu'ils sont corrigés** :

- **le décodeur rendait l'image d'avant** (C1) : la dernière image d'un écran qui s'arrête ne
  s'affichait jamais ;
- **l'hôte jetait sa première image et n'honorait une demande d'image clé qu'au prochain changement
  d'écran** (I1) : sur un écran figé au lancement, le spectateur attendait sans fin.

**Vie privée — à lire avant.** `--source ecran` transmet **ton écran réel** : ici à toi-même, sur la
même machine, mais le spectateur l'**écrit aussi sur ton disque** dans `spike/recu.h265`. Ferme ou
masque tout ce que tu ne veux pas voir enregistré, et **supprime `spike/recu.h265` après chaque
essai**.

**Montage — à lire avant, sinon l'écran n'est jamais figé.** Sur une seule machine, tout ce qui
bouge sur l'écran capturé l'empêche d'être « figé » : la fenêtre du spectateur elle-même (elle
afficherait sa propre image, en boucle), les terminaux qui impriment une ligne par seconde, le
pointeur de la souris. Il faut donc (déduit, non essayé) :

- **deux écrans** : l'hôte capture l'écran 0 (`--monitor 0`, l'ordre d'`EnumDisplayMonitors`, à
  confirmer à l'œil) ; la fenêtre du spectateur, les trois terminaux et le pointeur restent sur
  l'**autre** écran ;
- avec **un seul écran**, ces deux essais ne sont pas faisables sur une machine : les garder pour
  l'essai à deux machines (hôte sur le PC fixe, spectateur sur le portable).

- **D — taper un mot, puis s'arrêter (C1).** Sur l'écran capturé, ouvre le Bloc-notes, vide. Hôte :

  ```
  cd /d/skyshare/spike && cargo run -p sky-probe -- host --source ecran --monitor 0 --seconds 180
  ```

  > **⚠️ La commande suivante OUVRE UNE FENÊTRE** « SkyShare — écran de … » : place-la sur
  > l'**autre** écran.

  ```
  cd /d/skyshare/spike && ./target/release/sky-probe.exe view "NomDuSecondCompte" --seconds 150
  ```

  Une fois l'image affichée, clique dans le Bloc-notes, **tape un mot** (par exemple « bonjour »),
  ramène le pointeur sur l'autre écran, puis **ne touche plus à rien** pendant 30 s. Attendu
  (déduit) : le mot apparaît **en entier, dernière lettre comprise**, dans la fenêtre du
  spectateur, en une fraction de seconde, et y reste. Avant la correction, la dernière lettre
  manquait tant que l'écran ne bougeait plus. **Une lettre manquante est un défaut à signaler.**
  Répète avec un second mot.
- **E — écran figé au lancement (I1).** Laisse l'écran capturé **immobile** (rien qui clignote,
  pointeur sur l'autre écran). Hôte, puis spectateur :

  ```
  cd /d/skyshare/spike && cargo run -p sky-probe -- host --source ecran --monitor 0 --seconds 120
  ```

  > **⚠️ La commande suivante OUVRE UNE FENÊTRE** « SkyShare — écran de … » : place-la sur
  > l'**autre** écran.

  ```
  cd /d/skyshare/spike && ./target/release/sky-probe.exe view "NomDuSecondCompte" --seconds 60
  ```

  **Ne touche à rien.** Attendu (déduit) : la fenêtre quitte « En attente de l'image… » et montre
  l'écran capturé **sans que tu bouges quoi que ce soit**, puis le garde affiché jusqu'à la fin.
  Avant la correction, elle restait sur « En attente » jusqu'au premier changement d'écran. **Une
  attente qui dure est un défaut à signaler** ; un abandon au bout de dix secondes sur « L'image ne
  peut pas être reconstituée » aussi.

  *Ce qu'E ne couvre pas* : la reprise après une **vraie perte** de paquets suivie d'un écran figé
  (I1, second volet). Rien ne se perd sur une seule machine : seul l'essai à deux machines peut la
  voir.

Après D et E : **supprime `spike/recu.h265`**.

## 8. Deux points signalés par les revues, à surveiller

- **Une image clé arrivée avec des en-têtes inexploitables ne doit pas bloquer le spectateur.**
  Impossible à provoquer à la main. Le symptôme, s'il survenait : la fenêtre reste sur « En attente
  de l'image… » bien au-delà de dix secondes, alors que les lignes du terminal montrent des images
  reçues/s non nulles et `décodage 0.00 ms` (déduit du code : aucune image décodée dans le relevé).
  Si tu le vois, garde le texte du terminal.
- **Un arrêt annoncé ne doit pas s'afficher comme un échec** : c'est l'essai B.

## 9. Ce qu'il faut me rapporter

Le texte des terminaux — **déduit du code** : aucune adresse IP n'y figure tant qu'aucun collecteur
de traces n'est installé et que `RUST_LOG` n'est pas défini ; relis-les quand même avant de les
partager —, les cases cochées du §5, le tableau du §6 rempli — « non mesuré » partout où rien n'a
été relevé —, ce que tu as vu aux essais D et E (ou « non fait, un seul écran »), et tout message
d'erreur, même passager, recopié tel quel. Les résultats iront dans `spike/docs/mesures-jalon-2.md`.
