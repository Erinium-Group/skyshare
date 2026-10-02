# Essai du jalon 2 — fiche pour le propriétaire

Ce que l'essai prouve : **un pixel juste apparaît à l'écran** — couleur, netteté, états, plein
écran — et ce que coûte le décodage **pendant** un encodage sur la même carte. Il ne prouve rien du
réseau : l'écart 7 attend l'essai à deux machines sur deux réseaux.

Rédigée le 02/10/2026 en relisant le code (`sky-probe/src/main.rs`, `cmd_host.rs`, `cmd_view.rs`,
`sky-partage/src/spectateur.rs`), **sans rien exécuter**. Ce qui est écrit « déduit du code » n'a
jamais été observé.

**Toutes les commandes se tapent dans Git Bash**, une par bloc, dans l'ordre.

---

## ⛔ Pendant tout l'essai

**Ne jamais définir `RUST_LOG="str0m=debug"`** (ni aucun `RUST_LOG` visant `str0m`). Les traces
les plus bavardes de `str0m` écrivent l'adresse des deux machines **en clair** dans la console, donc
dans toute capture d'écran que tu partagerais ensuite. Le filtrage `pii` ne les couvre pas.

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

```
/d/skyshare/spike/target/release/sky-app.exe
```

À vérifier : la fenêtre s'ouvre, l'interface est remplie (pas « localhost a refusé de se
connecter »), l'icône apparaît près de l'horloge. Puis quitte-la par l'icône.

Après la construction, `git status` montre `spike/crates/sky-app/Cargo.toml` modifié : **c'est
normal**, la construction réécrit ses fins de ligne. `git diff --ignore-cr-at-eol` doit être vide.
Ne pas le commiter.

## 2. Construire `sky-probe` en `--release`

```
cd /d/skyshare/spike && cargo build --release -p sky-probe
```

L'exécutable présent date du 27/09 : il ne contient rien du jalon 2. **Toujours `--release`** : en
`debug`, `sky-probe` ouvre un coffre distinct (`SkyShare.dev`), vide ; s'y connecter avec ton
compte principal créerait un **second appareil sur ton compte réel**.

## 3. ⚠️ À trancher avant l'essai : il faut deux identités sur la machine

**Déduit du code, non essayé.** `view` ne peut regarder qu'un **ami** (`sky-compte/src/annuaire.rs:781`
cherche dans `etat.amis`) : ton propre compte n'en est pas un. Et une build `--release` n'a qu'une
identité par machine (`sky-compte/src/coffre.rs:104`). La commande prévue par le plan,
`view <nom-de-l-appareil>`, **ne peut donc pas fonctionner** telle quelle.

La seule seconde identité possible sur cette machine est le coffre de développement. D'où ce
montage, **qui déroge à la règle « jamais en `debug` »** et que toi seul peux accepter :

| Rôle | Build | Compte |
|---|---|---|
| hôte (`host`) | `debug` | ton **second** compte Discord (celui du portable au C2) |
| spectateur (`view`) | `--release` | ton compte principal |

Le spectateur est en `--release` parce que c'est lui qu'on mesure. Les chiffres de l'**hôte**, en
`debug`, ne sont pas représentatifs.

Préparation, une seule fois, **avec le second compte** :

```
cd /d/skyshare/spike && cargo run -p sky-probe -- login
```

Avant d'autoriser dans le navigateur, **lis le nom du compte affiché par Discord**. Si c'est ton
compte principal, n'autorise pas. À la fin, le terminal affiche « Connecté en tant que … » : si
c'est ton compte principal, arrête-toi et dis-le moi (un appareil de trop serait né sur ton compte
réel ; il se révoque depuis « Mon compte »).

```
cd /d/skyshare/spike && cargo run -p sky-probe -- device register "PC-fixe-essai"
```

```
cd /d/skyshare/spike && cargo run -p sky-probe -- friends list
```

Ton compte principal doit y figurer comme ami. Note le **nom Discord exact du second compte** : le
spectateur le désignera.

*Autre montage possible pour l'essai visuel seul* : le portable en spectateur, en `--release`, s'il
a une NVIDIA Turing ou plus récente (sans elle, il ne peut plus regarder). Mais la mesure du §6
exige les deux rôles **sur la même machine**.

## 4. Lancer un essai

Toujours dans cet ordre : l'hôte d'abord, le spectateur ensuite. Pendant l'attente, **aucune autre
commande `sky-probe`** (elle consommerait les messages de la négociation).

Hôte, avec l'image de test et non ton écran (le terminal de l'hôte affiche une colonne « RTT » à
**0,0 ms en dur** : ce n'est pas une mesure) :

```
cd /d/skyshare/spike && cargo run -p sky-probe -- host --source synthetique --seconds 90
```

> **⚠️ La commande suivante OUVRE UNE FENÊTRE** « SkyShare — écran de … », après la connexion.
> La fermer par la croix arrête le visionnage.

Spectateur (remplace le nom par celui du second compte, guillemets compris) :

```
cd /d/skyshare/spike && ./target/release/sky-probe.exe view "NomDuSecondCompte" --seconds 60
```

Le flux reçu est écrit dans `spike/recu.h265` (exclu par git). Il ne contient que l'image de test ;
supprime-le après l'essai.

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
  pixel doivent rester **distinctes**. Un violet uniforme et flou en
  plein écran signalerait une conscience DPI manquante (ou un 4:2:0). La source synthétique n'a pas
  de vrai texte : juger un vrai texte demanderait `--source ecran`, qui transmet ton écran réel —
  à ta discrétion.
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
imprime une ligne par relevé : `Mbps | images/s | gigue | décodage | images écartées`.

| Grandeur | Où la lire | Remarque |
|---|---|---|
| Images par seconde | ligne du spectateur | ce sont les images **arrivées**, pas affichées ; l'hôte en produit 60 (constante du code) |
| Latence de décodage | ligne du spectateur, « décodage » | une **moyenne** par relevé, pas une médiane ; la sonde avait mesuré 1,57 ms médiane, décodage seul |
| Latence capture → pixel | — | **aucun instrument du dépôt ne la mesure**. Le « transit » du bilan compare deux horloges d'origines différentes : ce n'est pas elle |
| Débit | ligne du spectateur, et bilan final | |
| Images écartées | ligne du spectateur | doit rester à 0 sur un essai sans perte |
| Occupation `enc` / `dec` | `nvidia-smi dmon` | à relever **pendant** l'essai |

**Décodage pendant un encodage (60 s).** L'essai A ci-dessous *est* cette mesure : sur la même
machine, l'hôte encode pendant que le spectateur décode. Relève, sur ses 60 secondes : images/s et
décodage côté spectateur, `enc` et `dec` dans `nvidia-smi dmon`, et si l'un des deux
**s'effondre**. Si cela ne tient pas, c'est une limite à écrire, pas un échec de l'essai.

## 7. Les trois essais

- **A — le principal (60 s, mesures et contrôles visuels).** Commandes du §4 telles quelles : hôte
  90 s, spectateur 60 s. Le spectateur finit sur son bilan (« Débit moyen reçu … »).
- **B — l'arrêt annoncé.** Hôte `--seconds 20`, spectateur `--seconds 120`. Attendu (déduit du
  code) : la fenêtre affiche « Le partage s'est arrêté », puis le terminal du spectateur
  « Ton ami a arrêté son partage. » — **et pas « ÉCHEC »**. Un « ÉCHEC » ici serait un défaut à
  signaler.
- **C — la connexion perdue.** Hôte `--seconds 300`, spectateur `--seconds 120`. Une fois l'image
  affichée, **Ctrl+C dans le terminal de l'hôte** (il s'arrête sans rien annoncer). Attendu
  (déduit) : « Connexion perdue » dans la fenêtre, « ÉCHEC : … » au terminal — ici c'est juste.
  Le délai avant que la perte soit vue n'est **pas mesuré** : le noter.

## 8. Deux points signalés par les revues, à surveiller

- **Une image clé arrivée avec des en-têtes inexploitables ne doit pas bloquer le spectateur.**
  Impossible à provoquer à la main. Le symptôme, s'il survenait : la fenêtre reste sur « En attente
  de l'image… » bien au-delà de dix secondes, alors que les lignes du terminal montrent des
  images/s non nulles et `décodage 0.00 ms` (déduit du code : aucune image décodée dans le relevé).
Si tu le vois, garde le texte du terminal.
- **Un arrêt annoncé ne doit pas s'afficher comme un échec** : c'est l'essai B.

## 9. Ce qu'il faut me rapporter

Le texte des terminaux (aucune adresse IP n'y figure tant que `RUST_LOG` n'est pas défini), les
cases cochées du §5, le tableau du §6 rempli — « non mesuré » partout où rien n'a été relevé — et
tout message d'erreur, même passager, recopié tel quel. Les résultats iront dans
`spike/docs/mesures-jalon-2.md`.
