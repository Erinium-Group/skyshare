# Jalon 1 — l'application SkyShare — plan d'implémentation

> **Pour les agents :** SOUS-COMPÉTENCE REQUISE — utiliser `superpowers:subagent-driven-development`
> (recommandé) ou `superpowers:executing-plans` pour exécuter ce plan tâche par tâche. Les étapes
> utilisent des cases à cocher (`- [ ]`).

**But :** une vraie application Windows (Tauri 2 + React) qui démarre avec la session, vit près de
l'horloge, remplace la ligne de commande pour le compte, les amis et les listes, et sait
**partager** et **regarder** — la connexion et ses mesures, sans image.

**Architecture :** la négociation quitte `sky-probe` pour une crate `sky-partage` (décision D5) qui
rend des événements typés et accepte un signal d'arrêt ; `sky-probe host`/`view` n'en sont plus que
des affichages. Une crate `sky-app` (le « src-tauri », membre du workspace) tient **seule** l'état :
un `Noyau` sans dépendance à Tauri, testable contre le serveur double, et une coquille Tauri mince
(fenêtre, icône, événements). L'interface React (`app/`) n'a ni jeton, ni clé, ni réseau : elle
affiche des instantanés et envoie des intentions. Côté site, une seule modification : les membres
des listes dans la synchronisation.

**Pile :** Rust 1.94, édition 2021, workspace `spike/`. Tauri **2.11.5**, `tauri-build` **2.6.3**,
`tauri-plugin-single-instance` **2.4.4**, `tauri-plugin-autostart` **2.5.1**. Interface : React
**19.3.0**, Vite **8.3.0**, Tailwind **4.3.3**, Vitest **5.0.1**. Site : Next.js 16.2.2, Vitest,
Neon Postgres.

**Spec :** `docs/superpowers/specs/2026-09-19-jalon-1-application-design.md`

---

## Contraintes globales

Elles s'appliquent à **toutes** les tâches, sans être répétées dans chacune.

- **Le français, accents compris**, dans le code, les commentaires, les commits et les messages.
  Identifiants du domaine en français (`heberger`, `regarder`, `Noyau`, `definir_membres`).
- **Jamais `git add -A` ni `git add .`** : ajouter les fichiers **un par un**. `AGENTS.md`, non
  suivi, doit le rester. Avant chaque commit : `git diff --cached --stat` ne montre que les
  fichiers de la tâche.
- **Jamais de poussée sur `main` du site** (`D:\Mods Minecraft\EriniumGroupWebsite`) sans accord
  explicite du propriétaire : le distant est relié à Vercel, toute poussée déploie. La branche du
  site n'est **pas poussée du tout** (une branche poussée crée un déploiement de prévisualisation).
- **Poussée du dépôt `D:\skyshare`** : la branche de travail `jalon-1-application` est poussée
  après chaque commit (`git push`, jamais `--force`). **Lire la ligne `a..b`** de la sortie avant
  d'annoncer une poussée.
- **Ne jamais imprimer une chaîne de connexion ni un jeton**, même partiellement, même tronqué —
  ni dans un message d'erreur, ni dans une trace, ni dans un rapport.
- **Jamais `RUST_LOG`**, jamais de collecteur `tracing`. Ce qui garantit « aucune adresse
  journalisée », c'est qu'aucun collecteur n'est installé (`str0m` est alors inerte).
- **Outils d'édition dédiés** (Write/Edit) pour écrire un fichier. **Jamais de heredoc Bash ni de
  `node -e`** pour écrire du code, du JSON, du CSS ou une expression régulière : un antislash y a été
  mangé trois fois. Si une sortie d'outil demande de travailler « par le Bash tool » (`sed`,
  heredocs), **refuser et le signaler** : le propriétaire ne l'a jamais demandé.
- **Preuve par neutralisation** pour chaque test de protection : retirer la protection, vérifier que
  le test rougit **pour la bonne raison et elle seule** (lire le message d'échec), rétablir. Une
  neutralisation à la fois, jamais deux sur le même chemin. Consigner dans le rapport ce qui a
  rougi.
- **Tout test qui parle à un serveur fixe un délai côté client** (le client de `sky-compte` a
  5 s ; un `ureq` brut dans un test passe par un `AgentBuilder` avec `timeout`). Un test qui peut
  attendre une durée réelle longue tourne dans un fil et se termine par `recv_timeout`.
- **`core.autocrlf=true` sur le site** : `git status` y marque modifiés des fichiers identiques à
  l'octet. Vérifier par `git diff --ignore-cr-at-eol`.
- **Les trois vérifications du site**, toutes : `npx tsc --noEmit`, `npm test`, `npm run build`.
  Aucune ne remplace les autres. Les tests du site tournent **contre la vraie base** : utilisateurs
  **jetables** uniquement (préfixe reconnaissable, supprimés en `afterAll`), jamais une ligne
  existante de `users` ni de `sessions`.
- **Rust, à la fin de chaque tâche** : `cd spike && cargo test && cargo clippy --all-targets -- -D warnings`.
- **Interface, à la fin de chaque tâche qui la touche** : `npm --prefix app test` et
  `npm --prefix app run build`.
- **Pas de runtime asynchrone** dans `sky-compte`, `sky-partage` ni dans le `Noyau` de `sky-app`.
  Seules exceptions : les fonctions `#[tauri::command] async fn` de `sky-app/src/commandes.rs`, qui
  ne font que confier le travail à `tauri::async_runtime::spawn_blocking` (un fil dédié, jamais le
  fil de l'interface — spec §3).
- **Types de l'API, à ne pas confondre :** `Enveloppe.id` est une **chaîne** ; les identifiants
  d'appareil, d'utilisateur, d'amitié et de liste sont des **entiers**. `friendshipId` (amitié) ≠
  `id` (utilisateur) : les routes `friends/{id}` lisent un identifiant d'**amitié**.
- **Constantes à ne pas changer :** alphabet du code ami, taille d'enveloppe 4096 octets,
  `CADENCE` 2 s, `FENETRE_HOTE` 30 min, `ATTENTE_SPECTATEUR` 60 s.

---

## Versions épinglées — comment elles ont été relevées (19/09/2026)

| Paquet | Version | Source |
|---|---|---|
| `tauri` (crate) | `=2.11.5`, fonctionnalités `tray-icon`, `image-png` | `cargo info tauri@2` (la dernière publiée est `3.0.0-alpha.1`, écartée : la spec fixe Tauri 2) ; noms de fonctionnalités lus par `cargo info tauri@2.11.5 -v` |
| `tauri-build` | `=2.6.3` | `cargo info tauri-build@2` |
| `tauri-plugin-single-instance` | `=2.4.4` | `cargo info tauri-plugin-single-instance@2` |
| `tauri-plugin-autostart` | `=2.5.1` | `cargo info tauri-plugin-autostart@2` |
| `@tauri-apps/cli` | `2.11.4` | `npm view @tauri-apps/cli version` |
| `@tauri-apps/api` | `2.11.1` | `npm view @tauri-apps/api version` |
| `react`, `react-dom`, `@types/react`, `@types/react-dom` | `19.3.0` | `npm view … version` |
| `vite` | `8.3.0` | `npm view vite version` |
| `@vitejs/plugin-react` | `6.1.1` (pair : `vite ^8`) | `npm view … version` / `peerDependencies` |
| `tailwindcss`, `@tailwindcss/vite` | `4.3.3` | `npm view … version` |
| `vitest` | `5.0.1` (pair : `vite ^6 \|\| ^7 \|\| ^8`) | `npm view … version` / `peerDependencies` |
| `jsdom` | `30.1.0` | `npm view jsdom version` |
| `@testing-library/react` | `16.3.3` | `npm view … version` |
| `@testing-library/dom` | `10.4.2` (pair exigé par la précédente) | `npm view … version` |
| `@testing-library/jest-dom` | `7.0.1` (sous-chemin `./vitest` vérifié dans `exports`) | `npm view … exports` |
| `@testing-library/user-event` | `14.6.7` | `npm view … version` |
| `typescript` | `5.9.3` | `npm view typescript@5 version` — **choix** : la dernière publiée est `7.0.2` (le compilateur réécrit) ; la 5.x évite d'être les premiers à découvrir une incompatibilité avec Vite/Vitest. |
| `@fontsource/manrope`, `@fontsource/instrument-serif` | `5.3.0` | `npm view … version` — polices embarquées, aucune requête réseau depuis l'application |

Faits vérifiés dans les sources téléchargées (`cargo fetch` d'un projet jetable dans le scratchpad) :
`tauri_plugin_autostart::init(MacosLauncher, Option<Vec<&'static str>>)` et le trait `ManagerExt`
(`app.autolaunch().enable() / disable() / is_enabled()`) ; l'instance unique Windows repose sur un
mutex nommé `{identifier}-sim` et fait `std::process::exit(0)` dans la seconde instance ; sans la
fonctionnalité `tauri/custom-protocol` (que `tauri build` ajoute lui-même), `tauri` compile en mode
développement et **n'exige pas** que `frontendDist` existe — `cargo test` à la racine du workspace ne
dépend donc pas d'un `npm run build` préalable ; `tauri::include_image!` résout ses chemins depuis
`CARGO_MANIFEST_DIR` ; `app.available_monitors()` (tao 0.35.3) énumère les écrans par
`EnumDisplayMonitors` dans le même ordre que `sky-capture` ; `cudarc` 0.16.6 **panique** (et ne rend
pas d'erreur) quand `nvcuda.dll` est absente — `probe_hardware()` doit être appelée sous
`catch_unwind` (tâche 11).

---

## Propriété des fichiers

Un fichier a **un** propriétaire. Les autres tâches n'y touchent que dans la limite écrite dans la
colonne « arbitrage ». Cette table existe parce qu'au jalon A, des fichiers sans propriétaire n'ont
été relus par personne.

| Fichier | Propriétaire | Arbitrage (qui d'autre y touche, et à quoi) |
|---|---|---|
| Site `src/lib/sky/listes.ts` | T1 | — |
| Site `src/lib/sky/etat.ts` | T1 | — |
| Site `src/lib/sky/__tests__/listes.test.ts` | T1 | — |
| Site `src/lib/sky/__tests__/etat.test.ts` | T1 | — |
| Site `docs/mesures-jalon-c1.md` | T1 | — |
| `spike/crates/sky-compte/src/annuaire.rs` | T2 | T3 : `Ami::friendship_id`, `retirer_ami`, `bloquer_ami`, `regenerer_code`, `pub` sur `revoquer_appareil` et `nom_appareil_valide` |
| `spike/crates/sky-compte/src/listes.rs` | T2 | — |
| `spike/crates/sky-compte/src/http.rs` | T2 | — |
| `spike/crates/sky-compte/src/lib.rs` | T2 | T3 : réexportations |
| `spike/crates/sky-compte/src/boite.rs` | — | T2 : ajout du seul champ `listes` au littéral de test `etat_avec` |
| `spike/crates/sky-compte/tests/annuaire_test.rs` | — | T2 : champ `listes` dans deux littéraux ; T3 : champ `friendship_id` dans un littéral |
| `spike/crates/sky-compte/tests/listes_test.rs` | T2 | — |
| `spike/crates/sky-compte/tests/amis_test.rs` | T3 | — |
| `spike/crates/sky-compte/tests/faux_serveur/mod.rs` | T2 | T3 : amis et code ; T7 : champ `syncs_recues` |
| `spike/crates/sky-compte/tests/faux_serveur/tests_du_double.rs` | T2 | — |
| `spike/crates/sky-probe/src/rendez_vous.rs` → `spike/crates/sky-partage/src/rendez_vous.rs` | T4 (déplacement) | T2 : champ `listes` ; T3 : champ `friendship_id` — ajouts mécaniques aux littéraux de test, rien d'autre, **avant** le déplacement |
| `spike/crates/sky-partage/**` | T4 (crate, `arret.rs`) | T5 : `evenement.rs`, `etablissement.rs`, `reception.rs`, `hote.rs`, `spectateur.rs`, `lib.rs`, `Cargo.toml` |
| `spike/crates/sky-probe/Cargo.toml`, `src/main.rs` | T4 | — |
| `spike/crates/sky-probe/src/cmd_host.rs`, `cmd_view.rs` | T5 | T4 : lignes `use` seulement |
| `spike/crates/sky-probe/src/cmd_encode.rs` | — | T5 : la seule ligne `const FPS` |
| `spike/Cargo.lock` | la tâche qui ajoute une dépendance (T4, T5, T6, T7, T11) | — |
| `spike/crates/sky-app/Cargo.toml`, `build.rs`, `tauri.conf.json`, `capabilities/default.json`, `icons/**` | T6 | T7 : dépendances `sky-compte`, `sky-partage` ; T11 : `sky-encode`, `icons/partage*` |
| `spike/crates/sky-app/src/lib.rs` | T6 | T7, T8, T10, T11 : déclarations de modules et liste de `generate_handler!` ; T7, T11 : `lancer()` |
| `spike/crates/sky-app/src/main.rs`, `src/demarrage.rs`, `tests/instance_unique.rs` | T6 | — |
| `spike/crates/sky-app/src/{cadence,reveil,vue,materiel,coquille,noyau,commandes,essais}.rs` | T7 | T8, T10 : méthodes de `Noyau` et commandes ; T11 : partage, `detecter_nvenc`, `icone_partage` |
| `spike/crates/sky-app/src/partage.rs` | T11 | — |
| `app/package.json`, `package-lock.json`, `index.html`, `vite.config.ts`, `tsconfig.json`, `src/main.tsx`, `src/styles.css`, `src/Disposition.tsx`, `src/test/installation.ts` | T6 | — |
| `app/src/App.tsx`, `src/App.test.tsx` | T6 | T8 : branchement de l'instantané ; T10, T12 : écrans et barre de partage |
| `app/src/{types,pont,useInstantane}.ts` | T8 | T10, T12 : ajouts de fonctions au `pont` |
| `app/src/ecrans/{Connexion,Amis}.tsx` et leurs tests | T8 | — |
| `app/src/test/fabriques.ts` | T8 | T10, T12 : aucune modification (ils l'importent) |
| `app/src/ecrans/{Listes,MonCompte}.tsx` et leurs tests | T10 | — |
| `app/src/{messages.ts,messages.test.ts}`, `app/src/composants/**` | T12 | — |
| `.gitignore` (racine de `D:\skyshare`) | T6 | — |
| `tasks/todo.md` | T9, T13 (résultats d'essai) | le contrôleur (état des tâches) |

---

## Découpage

Treize tâches, dans l'ordre imposé par le contrôleur. **Un seul affinage :** la tâche « extraction
`sky-partage` » est coupée en deux (T4 déplace le rendez-vous et pose l'arrêt ; T5 déplace
l'établissement, la diffusion et la réception). Le déplacement des tests (T4) est vérifiable à la
ligne près ; la transformation des boucles de `cmd_host`/`cmd_view` en événements (T5) est un autre
geste, qu'un relecteur doit pouvoir refuser sans défaire le premier. Les numéros suivants sont donc
décalés d'un cran par rapport à la liste du contrôleur.

| # | Tâche | Contrôleur |
|---|---|---|
| T1 | Site — membres des listes dans la synchronisation | 1 |
| T2 | `sky-compte` — listes | 2 |
| T3 | `sky-compte` — amis et compte | 3 |
| T4 | `sky-partage` — la crate, le rendez-vous déplacé, l'arrêt | 4 (1/2) |
| T5 | `sky-partage` — hôte et spectateur ; `sky-probe` en affichage | 4 (2/2) |
| T6 | Squelette de l'application | 5 |
| T7 | Cœur `sky-app` — état et synchronisation | 6 |
| T8 | Interface — Connexion et Amis | 7 |
| T9 | Point d'arrêt — premier essai réel (propriétaire) | 8 |
| T10 | Interface — Listes et Mon compte | 9 |
| T11 | Partage dans `sky-app` | 10 |
| T12 | Interface — panneau de partage et échecs | 11 |
| T13 | Essai réel final et installateur (propriétaire) | 12 |

**Où vit `src-tauri`.** Dans `spike/crates/sky-app/`, membre du workspace (`members = ["crates/*"]`
l'inclut sans modification). Raisons : un seul `Cargo.lock` et un seul `target/` ; `sky-compte` et
`sky-partage` en dépendances de chemin ; `cargo test` et `cargo clippy --all-targets` couvrent
l'application avec le reste — ce qu'un dossier `src-tauri` hors workspace perdrait en silence.
L'interface reste à la racine, `app/` (spec §3). `tauri.conf.json` vit à côté du `Cargo.toml` de
`sky-app` et désigne `../../../app/dist` (chemin relatif au fichier de configuration : vérifié dans
`tauri-codegen`, `config_parent.join(path)`). La CLI Tauri se lance **depuis
`spike/crates/sky-app`** ; aucune commande `beforeDevCommand`/`beforeBuildCommand` : le répertoire de
travail de ces crochets n'est pas documenté de façon vérifiable, le plan construit l'interface par
une commande explicite juste avant.

---

## Task 1 : Site — membres des listes dans la synchronisation

**Dépôt :** `D:\Mods Minecraft\EriniumGroupWebsite`, branche `jalon-1-membres-listes` créée depuis
`main`. **Ne jamais pousser** (ni la branche, ni `main`). La mise en production se fait au point
d'arrêt T9, sur accord du propriétaire, par le contrôleur.

**Ce qui manque (spec §5).** `listesDe` (`src/lib/sky/listes.ts:101`) ne sélectionne que
`id, nom, couleur, emoji, created_at` ; `definirMembres` écrit `list_members` et rien ne le relit.
L'écran Listes ne pourrait pas cocher les membres existants.

**Contrainte :** le nombre d'allers-retours d'une synchronisation complète ne change pas. Il est
aujourd'hui de **7** (relevé dans `etat.ts` : contrôle, `appareilsDe`, `amisDe`, `demandesDe`,
`listesDe`, `assurerCode` quand le code existe déjà, `releverPour` avec un appareil). Les membres sont
agrégés **dans la requête de `listesDe`**, par une sous-requête `json_agg` corrélée — le patron que
`amisDe` emploie déjà pour `appareils`.

**Files:**
- Modify: `src/lib/sky/listes.ts:96-106` (`listesDe`) — nouveau type `ListeAvecMembres`
- Modify: `src/lib/sky/etat.ts:3` et `:55` (type de `Etat.listes`)
- Modify: `src/lib/sky/__tests__/listes.test.ts` (un test, inséré avant la ligne 192)
- Modify: `src/lib/sky/__tests__/etat.test.ts` (deux tests, insérés après la ligne 287)
- Modify: `docs/mesures-jalon-c1.md` (poids mesuré après ajout de `membres`)

**Interfaces:**
- Consumes : `query` (`@/lib/db`), `definirMembres`, `creerListe` (inchangés).
- Produces (contrat réseau, lu par T2) : chaque élément de `GET /api/sky/sync` → `listes[]` a la
  forme `{ id: number, nom: string, couleur: string | null, emoji: string | null, created_at: string,
  membres: number[] }` ; `membres` = identifiants d'**utilisateurs**, triés croissants, `[]` pour une
  liste vide, **jamais absent**.
- Produces (TypeScript) : `export interface ListeAvecMembres extends Liste { membres: number[] }` ;
  `listesDe(userId: number): Promise<ListeAvecMembres[]>`.

- [ ] **Étape 1 : créer la branche**

```bash
cd "/d/Mods Minecraft/EriniumGroupWebsite" && git switch main && git switch -c jalon-1-membres-listes
```
Attendu : `Switched to a new branch 'jalon-1-membres-listes'`.

- [ ] **Étape 2 : écrire d'abord la garde du nombre de requêtes, et la voir passer sur le code actuel**

Dans `src/lib/sky/__tests__/etat.test.ts`, juste après la fin du test
`"une réponse inchange coûte strictement moins de requêtes qu'une réponse complète"` (ligne 287) :

```ts
    it("une réponse complète coûte exactement 7 requêtes, membres des listes compris", async () => {
      // Premier appel jeté : assurerCode coûte deux requêtes la première
      // fois que p n'a pas encore de code ami, une seule ensuite.
      await construireEtat(p.id, null, deviceP);

      queryEspion.mockClear();
      const complet = await construireEtat(p.id, null, deviceP);
      expect("inchange" in complet).toBe(false);

      // controle + appareilsDe + amisDe + demandesDe + listesDe
      // + assurerCode + releverPour([deviceP]). Les membres des listes
      // (jalon 1) doivent venir DANS la requête de listesDe : une requête
      // de plus ici changerait le budget de la synchronisation.
      expect(queryEspion.mock.calls.length).toBe(7);
    });
```

Lancer :
```bash
npx vitest run src/lib/sky/__tests__/etat.test.ts -t "exactement 7 requêtes"
```
Attendu : **PASS** sur le code actuel — c'est une garde, pas un test rouge. **Si le compte mesuré
n'est pas 7, s'arrêter et rapporter le nombre mesuré** au contrôleur : ne pas ajuster le chiffre du
test pour le faire passer.

- [ ] **Étape 3 : écrire les tests qui échouent**

Dans `src/lib/sky/__tests__/listes.test.ts`, juste **avant**
`it("creerListe rejette un nom deja pris par une AUTRE liste du meme utilisateur, …` (ligne 192) :

```ts
    it("listesDe rend les membres de chaque liste, dans la meme requete", async () => {
      const listeBC = await listeJetable(a, "Membres BC");
      const listeC = await listeJetable(a, "Membres C");
      const listeVide = await listeJetable(a, "Membres aucun");
      expect(await definirMembres(a.id, listeBC.id, [c.id, b.id])).toBe(true);
      expect(await definirMembres(a.id, listeC.id, [c.id])).toBe(true);

      queryEspion.mockClear();
      const listes = await listesDe(a.id);
      // Une seule requete : les membres sont agreges, pas relus a part.
      expect(queryEspion).toHaveBeenCalledTimes(1);

      const parId = new Map(listes.map((l) => [l.id, l]));
      expect(parId.get(listeBC.id)?.membres).toEqual([b.id, c.id].sort((x, y) => x - y));
      // Chaque liste porte SES membres : sans la correlation
      // `lm.list_id = fl.id`, listeC recevrait aussi b.
      expect(parId.get(listeC.id)?.membres).toEqual([c.id]);
      expect(parId.get(listeVide.id)?.membres).toEqual([]);
    });
```

Dans `src/lib/sky/__tests__/etat.test.ts`, juste après le test ajouté à l'étape 2 :

```ts
    it("chaque liste de la synchronisation porte ses membres", async () => {
      const etat = (await construireEtat(p.id, null, deviceP)) as Etat;
      const listeP = etat.listes.find((l) => l.nom === "Liste de P");
      // beforeAll : definirMembres(idP, listeP.id, [idQ]).
      expect(listeP?.membres).toEqual([q.id]);
      for (const liste of etat.listes) expect(Array.isArray(liste.membres)).toBe(true);
    });
```

- [ ] **Étape 4 : les lancer, vérifier l'échec**

```bash
npx vitest run src/lib/sky/__tests__/listes.test.ts src/lib/sky/__tests__/etat.test.ts
```
Attendu : les deux nouveaux tests **FAIL** — `membres` vaut `undefined` ; `npx tsc --noEmit` signale
aussi `Property 'membres' does not exist on type 'Liste'`. La garde de l'étape 2 reste verte.

- [ ] **Étape 5 : implémenter**

Dans `src/lib/sky/listes.ts`, remplacer le bloc de `listesDe` (lignes 96 à 106) par :

```ts
/**
 * Une liste telle que `listesDe` la rend : ses colonnes publiques PLUS les
 * identifiants d'utilisateur de ses membres (jalon 1 de l'application,
 * spec §5). Sans eux, l'ecran Listes ne pourrait pas afficher les cases
 * deja cochees : `definirMembres` ecrivait `list_members`, rien ne le
 * relisait.
 */
export interface ListeAvecMembres extends Liste {
  membres: number[];
}

/**
 * Rend les listes de l'utilisateur, jamais celles d'un autre — la clause
 * `user_id = $1` est la seule protection necessaire, aucune route
 * n'accepte d'identifiant de liste en entree de cette fonction.
 *
 * Les membres sont agreges DANS LA MEME requete (sous-requete `json_agg`
 * correlee, meme patron que `appareils` dans `amisDe`) : la synchronisation
 * garde son nombre d'allers-retours, fige a 7 par etat.test.ts. Les lire
 * par une seconde requete le ferait passer a 8. `ORDER BY membre_id` rend
 * un ordre stable ; `'[]'::json` rend un tableau vide, jamais `null`, pour
 * une liste sans membre.
 */
export async function listesDe(userId: number): Promise<ListeAvecMembres[]> {
  return query<ListeAvecMembres>(
    `SELECT fl.id, fl.nom, fl.couleur, fl.emoji, fl.created_at,
            COALESCE(
              (SELECT json_agg(lm.membre_id ORDER BY lm.membre_id)
                 FROM list_members lm
                WHERE lm.list_id = fl.id),
              '[]'::json
            ) AS membres
       FROM friend_lists fl
      WHERE fl.user_id = $1
      ORDER BY fl.created_at DESC`,
    [userId],
  );
}
```

Dans `src/lib/sky/etat.ts`, ligne 3 :
```ts
import { listesDe, type ListeAvecMembres } from "@/lib/sky/listes";
```
et ligne 55 :
```ts
  listes: ListeAvecMembres[];
```

- [ ] **Étape 6 : lancer les tests ciblés**

```bash
npx vitest run src/lib/sky/__tests__/listes.test.ts src/lib/sky/__tests__/etat.test.ts
```
Attendu : PASS, dont les trois tests de cette tâche.

- [ ] **Étape 7 : prouver chaque test par neutralisation (une à la fois)**

1. Remplacer la sous-requête par une seconde requête dans `listesDe` (par exemple
   `const lignes = await query(...colonnes seules...)` puis un `query` sur `list_members` pour
   remplir `membres`). Attendu : `exactement 7 requêtes` rougit (**8**) et
   `listesDe rend les membres…` rougit (`toHaveBeenCalledTimes(1)` : 2). Rien d'autre. Rétablir.
2. Retirer `WHERE lm.list_id = fl.id` de la sous-requête. Attendu : `listesDe rend les membres…`
   rougit sur `listeC` (elle reçoit aussi `b`) et `chaque liste de la synchronisation…` rougit.
   Rétablir.
3. Retirer `COALESCE(…, '[]'::json)` (garder la sous-requête nue). Attendu : `listeVide` rend
   `null` au lieu de `[]` — `listesDe rend les membres…` rougit sur la dernière assertion. Rétablir.

- [ ] **Étape 8 : les trois vérifications**

```bash
npx tsc --noEmit && npm test && npm run build
```
Attendu : aucune erreur de type ; tous les tests verts (le total d'avant la tâche — le relever au premier `npm test` — plus 3) ;
construction réussie.

- [ ] **Étape 9 : consigner le poids mesuré**

```bash
npx vitest run src/app/api/sky/sync/__tests__/route.test.ts --reporter=verbose
```
Relever dans la sortie les deux lignes `[poids representatif] … octets` et
`[poids realiste] … octets`. Ajouter à la fin de `docs/mesures-jalon-c1.md` la section suivante, en
y recopiant **les deux nombres mesurés** (et la date) — pas une estimation :

```markdown
## Jalon 1 de l'application — `listes[].membres` (tâche 1)

Chaque liste porte désormais `membres: number[]`, agrégé dans la requête de `listesDe` : le nombre
de requêtes d'une synchronisation complète reste **7** (figé par `etat.test.ts`, « exactement 7
requêtes »). Seul le poids change. Mesuré par `sync/__tests__/route.test.ts` (même fixture que
ci-dessus : 10 amis, 3 listes, 2 appareils) :

| Cas | Avant | Après (mesuré le JJ/MM/2026) |
|---|---|---|
| 10 amis sans appareil | 2 219 octets | (nombre de la ligne `[poids representatif]`) |
| 10 amis avec un appareil chacun | 2 929 octets | (nombre de la ligne `[poids realiste]`) |
```

Remplacer les deux parenthèses et `JJ/MM` par les valeurs lues. Si les lignes `[poids …]`
n'apparaissent pas dans la sortie, le dire dans le rapport plutôt que d'écrire un chiffre.

- [ ] **Étape 10 : commiter (sans pousser)**

```bash
git add src/lib/sky/listes.ts
git add src/lib/sky/etat.ts
git add src/lib/sky/__tests__/listes.test.ts
git add src/lib/sky/__tests__/etat.test.ts
git add docs/mesures-jalon-c1.md
git diff --cached --stat --ignore-cr-at-eol
git commit -m "feat: les listes de la synchronisation portent leurs membres, dans la meme requete"
```
Attendu : cinq fichiers au `--stat`. **Ne pas pousser.**

---

## Task 2 : `sky-compte` — listes

**Dépôt :** `D:\skyshare`, branche `jalon-1-application` (la créer depuis `main` si elle n'existe
pas : `git switch -c jalon-1-application`).

**Files:**
- Modify: `spike/crates/sky-compte/src/annuaire.rs` (type `Liste`, champ `Etat::listes`, `EtatBrut`, `convertir_etat`, littéraux de test lignes 689-698, 751-765, 792)
- Create: `spike/crates/sky-compte/src/listes.rs`
- Modify: `spike/crates/sky-compte/src/http.rs` (méthode `envoyer_json_reponse_vide_avec_refus` + test)
- Modify: `spike/crates/sky-compte/src/lib.rs`
- Modify: `spike/crates/sky-compte/src/boite.rs:227` (littéral de test)
- Modify: `spike/crates/sky-compte/tests/annuaire_test.rs:19` et `:95-108` (littéraux)
- Modify: `spike/crates/sky-probe/src/rendez_vous.rs:291-298` (littéral de test)
- Modify: `spike/crates/sky-compte/tests/faux_serveur/mod.rs`
- Modify: `spike/crates/sky-compte/tests/faux_serveur/tests_du_double.rs`
- Create: `spike/crates/sky-compte/tests/listes_test.rs`

**Interfaces:**
- Consumes : `ClientHttp::{post_json_avec_refus, delete_reponse_vide_avec_refus}`, `ReponseHttp`,
  `avec_jeton_valide`, `Config`, `Coffre`, `ErreurCompte` (existants, relevés dans `http.rs`,
  `session.rs`).
- Produces :
  - `pub struct Liste { pub id: i64, pub nom: String, pub couleur: Option<String>, pub emoji: Option<String>, pub created_at: String, pub membres: Vec<i64> }` (`#[derive(Debug, Clone, PartialEq)]`)
  - `Etat` gagne `pub listes: Vec<Liste>` (entre `demandes` et `appareils`)
  - `ClientHttp::envoyer_json_reponse_vide_avec_refus<B: Serialize>(&self, methode: &str, chemin: &str, corps: &B, jeton: Option<&str>) -> Result<ReponseHttp<()>, ErreurCompte>`
  - Dans `sky_compte::listes` (réexporté à la racine) :
    - `pub fn nom_liste_valide(nom: &str) -> bool`, `pub fn couleur_valide(couleur: Option<&str>) -> bool`, `pub fn emoji_valide(emoji: Option<&str>) -> bool`, `pub fn membres_valides(membres: &[i64]) -> bool`, `pub const MEMBRES_MAX: usize = 200`
    - `pub enum CreationListe { Creee(Liste), NomDejaPris }`
    - `pub enum ModificationListe { Modifiee, Introuvable, NomDejaPris }`
    - `pub enum SuppressionListe { Supprimee, Introuvable }`
    - `pub enum DefinitionMembres { Definis, Refuses }`
    - `pub struct ChampsListe<'a> { pub nom: Option<&'a str>, pub couleur: Option<Option<&'a str>>, pub emoji: Option<Option<&'a str>> }` (`Default`)
    - `pub fn creer_liste(config: &Config, coffre: &Coffre, nom: &str, couleur: Option<&str>, emoji: Option<&str>) -> Result<CreationListe, ErreurCompte>`
    - `pub fn modifier_liste(config: &Config, coffre: &Coffre, id: i64, champs: &ChampsListe<'_>) -> Result<ModificationListe, ErreurCompte>`
    - `pub fn supprimer_liste(config: &Config, coffre: &Coffre, id: i64) -> Result<SuppressionListe, ErreurCompte>`
    - `pub fn definir_membres(config: &Config, coffre: &Coffre, id: i64, membres: &[i64]) -> Result<DefinitionMembres, ErreurCompte>`
  - Serveur double : `pub struct ListeFausse { pub id, pub nom, pub couleur, pub emoji, pub created_at, pub membres }`, champs `EtatFaux::{listes, prochain_id_liste, appels_listes}`, fonction `identifiant_de_chemin(chemin, prefixe, suffixe) -> Option<i64>`.

**Les bornes, relevées dans le site (lecture seule) — le client et le double les appliquent
toutes, ni plus ni moins :**

| Paramètre | Ligne du site qui le lit | Règle |
|---|---|---|
| `nom` (POST, PATCH) | `lists/route.ts:18-25`, `lists/[id]/route.ts:21-28` | chaîne, `length` (unités UTF-16) de 1 à 40, `texteStockable` (pas d'octet NUL) |
| `couleur` | `lists/route.ts:35-38`, `lists/[id]/route.ts:30-37` | absente ou `null`, sinon `/^#[0-9A-Fa-f]{6}$/` |
| `emoji` | `lists/route.ts:52-59`, `lists/[id]/route.ts:39-46` | absent ou `null`, sinon `Buffer.byteLength ≤ 8` et pas d'octet NUL |
| corps PATCH | `lists/[id]/route.ts:85-115` | clés présentes seulement (`"cle" in donnees`) ; aucune clé → 400 |
| `membreIds` | `lists/[id]/members/route.ts:7-13, 54` | tableau d'au plus 200 entiers > 0 ; puis `definirMembres` : ≤ 2147483647, liste à l'appelant, **tous** amis acceptés — sinon **400 uniforme** « Liste introuvable ou un ou plusieurs identifiants ne sont pas des amis acceptes » |
| `id` de liste | `lists/[id]/route.ts:60-65` | entier > 0, sinon 400 ; au-delà d'INTEGER ou pas à l'appelant : 404 « Liste introuvable » (PATCH, DELETE) |
| nom déjà pris | `lists/route.ts:112-114`, `lists/[id]/route.ts:128-130` | 409 « Nom de liste deja utilise » |
| réponse POST | `lists/route.ts:110`, `listes.ts:85` (`RETURNING COLONNES_LISTE`) | 201 `{id, nom, couleur, emoji, created_at}` — **sans** `membres` |
| réponse PATCH, DELETE, PUT | `lists/[id]/route.ts:126,161`, `members/route.ts:89` | 204 sans corps |

**Deux comportements du site que le double reproduit exprès.** (1) Chaque écriture sur une liste
fait progresser la version (`friend_lists.updated_at`, `etat.ts:204`) — **sauf la suppression** :
la ligne disparaît, et la version (`MAX` des `updated_at` restants) peut ne pas bouger. Un client
qui synchronise avec `?version=` peut alors ne jamais voir une liste supprimée. Le double ne fait
donc **pas** progresser `version` sur `DELETE` : c'est ce qui rend significatif le test de
resynchronisation complète de T7/T8. (2) La réponse à la création ne porte pas `membres`.

- [ ] **Étape 1 : écrire les tests du double qui échouent**

À la fin du module `tests` de `spike/crates/sky-compte/tests/faux_serveur/tests_du_double.rs`
(avant son `}` final) :

```rust
    // --- Listes (jalon 1, tâche 2) : le double n'est jamais plus permissif
    // que les routes `src/app/api/sky/lists/**` du site. ---------------------

    /// Requête brute authentifiée, avec délai côté client (un test qui parle à
    /// un serveur ne doit jamais pouvoir pendre).
    fn requete_brute(s: &FauxServeur, methode: &str, chemin: &str, corps: &str) -> (u16, String) {
        let jeton = s.jeton_de_test();
        let agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(5)).build();
        match agent
            .request(methode, &format!("{}{chemin}", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .send_string(corps)
        {
            Ok(r) => (r.status(), r.into_string().unwrap_or_default()),
            Err(ureq::Error::Status(statut, r)) => (statut, r.into_string().unwrap_or_default()),
            Err(e) => panic!("échec de transport inattendu : {e}"),
        }
    }

    #[test]
    fn le_double_refuse_un_nom_de_liste_que_le_site_refuse() {
        // Site : `valeur.length >= 1 && valeur.length <= NOM_MAX && texteStockable`
        // (lists/route.ts:18-25). 20 émojis = 40 unités UTF-16.
        let s = FauxServeur::demarrer();
        let nom_41 = format!("{}a", "😀".repeat(20));
        for corps in [json!({"nom": nom_41}), json!({"nom": ""}), json!({"nom": "a\u{0}b"}), json!({"nom": 12})] {
            assert_eq!(requete_brute(&s, "POST", "/api/sky/lists", &corps.to_string()).0, 400, "corps {corps}");
        }
        assert_eq!(requete_brute(&s, "POST", "/api/sky/lists", &json!({"nom": "😀".repeat(20)}).to_string()).0, 201);
    }

    #[test]
    fn le_double_refuse_une_couleur_ou_un_emoji_que_le_site_refuse() {
        // Site : `/^#[0-9A-Fa-f]{6}$/` et `Buffer.byteLength(valeur) <= 8`
        // (lists/route.ts:37, 56). « 🇫🇷 » pèse 8 octets.
        let s = FauxServeur::demarrer();
        for corps in [
            json!({"nom": "A", "couleur": "#12345G"}),
            json!({"nom": "B", "couleur": "123456"}),
            json!({"nom": "C", "emoji": "🇫🇷a"}),
            json!({"nom": "D", "emoji": "\u{0}"}),
        ] {
            assert_eq!(requete_brute(&s, "POST", "/api/sky/lists", &corps.to_string()).0, 400, "corps {corps}");
        }
        let (statut, _) =
            requete_brute(&s, "POST", "/api/sky/lists", &json!({"nom": "E", "couleur": "#a1B2c3", "emoji": "🇫🇷"}).to_string());
        assert_eq!(statut, 201);
    }

    #[test]
    fn le_double_refuse_les_membres_d_un_400_uniforme() {
        // Site : un seul 400 pour « liste pas à toi » et « pas un ami accepté »
        // (members/route.ts:69-86). Même statut ET même corps.
        let s = FauxServeur::demarrer();
        s.etat_mut().amis.push(AmiFaux::sans_appareil("bob"));
        let bob = s.etat_mut().amis[0].id;
        let (statut, corps) = requete_brute(&s, "POST", "/api/sky/lists", r#"{"nom":"L"}"#);
        assert_eq!(statut, 201);
        let id = serde_json::from_str::<serde_json::Value>(&corps).unwrap()["id"].as_i64().unwrap();

        let non_ami = requete_brute(&s, "PUT", &format!("/api/sky/lists/{id}/members"), r#"{"membreIds":[424242]}"#);
        let liste_inconnue =
            requete_brute(&s, "PUT", "/api/sky/lists/999/members", &format!(r#"{{"membreIds":[{bob}]}}"#));
        assert_eq!(non_ami.0, 400);
        assert_eq!(non_ami, liste_inconnue, "les deux refus doivent être indiscernables");

        let trop: Vec<i64> = (1..=201).collect();
        let (statut, _) = requete_brute(&s, "PUT", &format!("/api/sky/lists/{id}/members"), &json!({"membreIds": trop}).to_string());
        assert_eq!(statut, 400);
        let (statut, _) = requete_brute(&s, "PUT", &format!("/api/sky/lists/{id}/members"), &format!(r#"{{"membreIds":[{bob}]}}"#));
        assert_eq!(statut, 204);
    }

    #[test]
    fn le_double_ne_fait_pas_progresser_la_version_sur_une_suppression_de_liste() {
        // Site : la ligne disparaît, la version est le MAX des `updated_at`
        // RESTANTS (etat.ts:200-209) — elle peut ne pas bouger.
        let s = FauxServeur::demarrer();
        let (_, corps) = requete_brute(&s, "POST", "/api/sky/lists", r#"{"nom":"L"}"#);
        let id = serde_json::from_str::<serde_json::Value>(&corps).unwrap()["id"].as_i64().unwrap();
        let version = s.etat_mut().version;
        assert_eq!(requete_brute(&s, "DELETE", &format!("/api/sky/lists/{id}"), "").0, 204);
        assert_eq!(s.etat_mut().version, version);
    }
```

- [ ] **Étape 2 : les lancer, vérifier l'échec**

```bash
cd spike && cargo test -p sky-compte --test faux_serveur_test
```
Attendu : les quatre nouveaux tests **FAIL** — le double répond `404 route inconnue du double`.

- [ ] **Étape 3 : étendre le double**

Dans `spike/crates/sky-compte/tests/faux_serveur/mod.rs` :

(a) Après `pub struct MoiFaux` et son `impl`, ajouter :

```rust
/// Liste telle que rendue par `GET /api/sky/sync` dans `listes[]` — forme
/// `ListeAvecMembres` du site (`src/lib/sky/listes.ts`, `listesDe`, jalon 1
/// tâche 1). `membres` : identifiants d'utilisateurs, triés croissants
/// (`json_agg(... ORDER BY membre_id)`), `[]` jamais absent.
#[derive(Debug, Clone, Serialize)]
pub struct ListeFausse {
    pub id: i64,
    pub nom: String,
    pub couleur: Option<String>,
    pub emoji: Option<String>,
    pub created_at: String,
    pub membres: Vec<i64>,
}
```

(b) À la fin de `pub struct EtatFaux`, avant son `}` :

```rust
    /// Listes de « l'utilisateur » du double (jalon 1, tâche 2).
    pub listes: Vec<ListeFausse>,
    /// Prochain identifiant rendu par `POST /api/sky/lists` (premier : 1).
    pub prochain_id_liste: i64,
    /// Nombre de requêtes reçues sur `/api/sky/lists*`, refusées comprises —
    /// prouve qu'une entrée invalide est refusée AVANT tout appel réseau.
    pub appels_listes: u64,
```

(c) Dans `repondre`, remplacer `if matches!(methode, Method::Post) {` par :

```rust
    if matches!(methode, Method::Post | Method::Put | Method::Patch) {
```

(d) Dans le `match` de `repondre`, juste avant la ligne `_ => (404u16, …`, ajouter :

```rust
        (Method::Post, "/api/sky/lists") => gerer_creer_liste(etat, jeton.as_deref(), &corps_brut),
        (Method::Put, chemin_membres)
            if chemin_membres.starts_with("/api/sky/lists/") && chemin_membres.ends_with("/members") =>
        {
            gerer_definir_membres(etat, jeton.as_deref(), chemin_membres, &corps_brut)
        }
        (Method::Patch, chemin_liste) if chemin_liste.starts_with("/api/sky/lists/") => {
            gerer_modifier_liste(etat, jeton.as_deref(), chemin_liste, &corps_brut)
        }
        (Method::Delete, chemin_liste) if chemin_liste.starts_with("/api/sky/lists/") => {
            gerer_supprimer_liste(etat, jeton.as_deref(), chemin_liste)
        }
```

(e) Dans `gerer_sync`, remplacer `"listes": Vec::<Value>::new(),` par :

```rust
        "listes": e.listes,
```

(f) À la fin du fichier (avant le commentaire final sur `tests_du_double.rs`), ajouter :

```rust
// --- Listes (jalon 1, tâche 2) ------------------------------------------
//
// Chaque règle est celle d'une ligne des routes `src/app/api/sky/lists/**`
// du site, relevée dans le plan du jalon 1 (tâche 2, tableau des bornes).
// Recopiée, pas importée : ce double ne dépend d'aucun code du site, ni de
// `sky_compte::listes` (il imiterait alors le client au lieu du site).

/// Borne d'un `INTEGER` Postgres — `POSTGRES_INTEGER_MAX` de `listes.ts`.
const INTEGER_POSTGRES_MAX: i64 = 2_147_483_647;
const NOM_LISTE_MAX: usize = 40;
const EMOJI_LISTE_OCTETS_MAX: usize = 8;
const MEMBRES_LISTE_MAX: usize = 200;
const REFUS_MEMBRES: &str =
    "Liste introuvable ou un ou plusieurs identifiants ne sont pas des amis acceptes";

/// Identifiant entier strictement positif entre `prefixe` et `suffixe` —
/// `Number(id)` puis `Number.isInteger(x) && x > 0` côté site. Plus strict
/// que le site (« 7.0 » refusé ici), jamais plus permissif.
fn identifiant_de_chemin(chemin: &str, prefixe: &str, suffixe: &str) -> Option<i64> {
    chemin.strip_prefix(prefixe)?.strip_suffix(suffixe)?.parse::<i64>().ok().filter(|id| *id > 0)
}

/// `nomValide` (lists/route.ts:18-25) : chaîne, 1 à 40 unités UTF-16, sans NUL.
fn nom_de_liste(valeur: &Value) -> Option<&str> {
    let nom = valeur.as_str()?;
    let unites = nom.encode_utf16().count();
    ((1..=NOM_LISTE_MAX).contains(&unites) && !nom.contains('\0')).then_some(nom)
}

/// `couleurValide` (lists/route.ts:35-38) : absente ou `null`, sinon `#RRGGBB`.
fn couleur_de_liste(valeur: Option<&Value>) -> Result<Option<String>, ()> {
    match valeur {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(c))
            if c.len() == 7 && c.starts_with('#') && c[1..].bytes().all(|o| o.is_ascii_hexdigit()) =>
        {
            Ok(Some(c.clone()))
        }
        _ => Err(()),
    }
}

/// `emojiValide` (lists/route.ts:52-59) : absent ou `null`, sinon 8 octets
/// UTF-8 au plus, sans NUL.
fn emoji_de_liste(valeur: Option<&Value>) -> Result<Option<String>, ()> {
    match valeur {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(e)) if e.len() <= EMOJI_LISTE_OCTETS_MAX && !e.contains('\0') => Ok(Some(e.clone())),
        _ => Err(()),
    }
}

fn refus(statut: u16, message: &str) -> (u16, String) {
    (statut, json!({ "error": message }).to_string())
}

/// `POST /api/sky/lists` — 201 avec les colonnes publiques SANS `membres`
/// (`creerListe` : `RETURNING COLONNES_LISTE`), 400 sur forme, 409 sur nom
/// déjà pris. La version progresse (`updated_at` posé à l'insertion).
fn gerer_creer_liste(etat: &Arc<Mutex<EtatFaux>>, jeton: Option<&str>, corps_brut: &str) -> (u16, String) {
    let mut e = etat.lock().expect("mutex etat faux empoisonne");
    e.appels_listes += 1;
    if let Some(r) = autoriser_appel(&mut e, jeton) {
        return r;
    }
    if e.session_partielle_totp {
        return refus(403, "Verification TOTP requise");
    }
    let corps: Value = match serde_json::from_str(corps_brut) {
        Ok(v @ Value::Object(_)) => v,
        _ => return refus(400, "Corps JSON invalide"),
    };
    let Some(nom) = corps.get("nom").and_then(nom_de_liste) else {
        return refus(400, "nom invalide : attendu 1 a 40 caracteres");
    };
    let Ok(couleur) = couleur_de_liste(corps.get("couleur")) else {
        return refus(400, "couleur invalide : attendu #RRGGBB ou null");
    };
    let Ok(emoji) = emoji_de_liste(corps.get("emoji")) else {
        return refus(400, "emoji invalide : attendu au plus 8 octets ou null");
    };
    if e.listes.iter().any(|l| l.nom == nom) {
        return refus(409, "Nom de liste deja utilise");
    }
    e.prochain_id_liste += 1;
    let liste = ListeFausse {
        id: e.prochain_id_liste,
        nom: nom.to_string(),
        couleur,
        emoji,
        created_at: "2026-01-01T00:00:00.000Z".to_string(),
        membres: Vec::new(),
    };
    e.listes.push(liste.clone());
    e.version += 1;
    let reponse = json!({
        "id": liste.id, "nom": liste.nom, "couleur": liste.couleur,
        "emoji": liste.emoji, "created_at": liste.created_at,
    });
    (201, reponse.to_string())
}

/// `PATCH /api/sky/lists/{id}` — mise à jour PARTIELLE : seules les clés
/// présentes sont validées et écrites (`"cle" in donnees`) ; `null` remet
/// couleur ou émoji à rien. Aucune clé : 400. Liste inconnue : 404.
fn gerer_modifier_liste(etat: &Arc<Mutex<EtatFaux>>, jeton: Option<&str>, chemin: &str, corps_brut: &str) -> (u16, String) {
    let mut e = etat.lock().expect("mutex etat faux empoisonne");
    e.appels_listes += 1;
    if let Some(r) = autoriser_appel(&mut e, jeton) {
        return r;
    }
    if e.session_partielle_totp {
        return refus(403, "Verification TOTP requise");
    }
    let Some(id) = identifiant_de_chemin(chemin, "/api/sky/lists/", "") else {
        return refus(400, "Identifiant invalide");
    };
    let objet = match serde_json::from_str::<Value>(corps_brut) {
        Ok(Value::Object(o)) => o,
        _ => return refus(400, "Corps JSON invalide"),
    };
    let mut nom = None;
    let mut couleur = None;
    let mut emoji = None;
    if let Some(v) = objet.get("nom") {
        match nom_de_liste(v) {
            Some(n) => nom = Some(n.to_string()),
            None => return refus(400, "nom invalide : attendu 1 a 40 caracteres"),
        }
    }
    if objet.contains_key("couleur") {
        match couleur_de_liste(objet.get("couleur")) {
            Ok(c) => couleur = Some(c),
            Err(()) => return refus(400, "couleur invalide : attendu #RRGGBB ou null"),
        }
    }
    if objet.contains_key("emoji") {
        match emoji_de_liste(objet.get("emoji")) {
            Ok(em) => emoji = Some(em),
            Err(()) => return refus(400, "emoji invalide : attendu au plus 8 octets ou null"),
        }
    }
    if nom.is_none() && couleur.is_none() && emoji.is_none() {
        return refus(400, "Aucun champ a modifier");
    }
    let Some(index) = e.listes.iter().position(|l| l.id == id) else {
        return refus(404, "Liste introuvable");
    };
    if let Some(n) = &nom {
        if e.listes.iter().any(|l| l.id != id && &l.nom == n) {
            return refus(409, "Nom de liste deja utilise");
        }
    }
    let liste = &mut e.listes[index];
    if let Some(n) = nom {
        liste.nom = n;
    }
    if let Some(c) = couleur {
        liste.couleur = c;
    }
    if let Some(em) = emoji {
        liste.emoji = em;
    }
    e.version += 1;
    (204, String::new())
}

/// `DELETE /api/sky/lists/{id}` — 204, ou 404 « Liste introuvable ». NE fait
/// PAS progresser `version` : voir le plan du jalon 1, tâche 2.
fn gerer_supprimer_liste(etat: &Arc<Mutex<EtatFaux>>, jeton: Option<&str>, chemin: &str) -> (u16, String) {
    let mut e = etat.lock().expect("mutex etat faux empoisonne");
    e.appels_listes += 1;
    if let Some(r) = autoriser_appel(&mut e, jeton) {
        return r;
    }
    if e.session_partielle_totp {
        return refus(403, "Verification TOTP requise");
    }
    let Some(id) = identifiant_de_chemin(chemin, "/api/sky/lists/", "") else {
        return refus(400, "Identifiant invalide");
    };
    let avant = e.listes.len();
    e.listes.retain(|l| l.id != id);
    if e.listes.len() == avant {
        return refus(404, "Liste introuvable");
    }
    (204, String::new())
}

/// `PUT /api/sky/lists/{id}/members` — `membreIds` : tableau d'au plus 200
/// entiers > 0 (400 sinon) ; puis `definirMembres` : borne INTEGER, liste à
/// l'appelant, TOUS amis acceptés, sinon 400 UNIFORME ; succès : ensemble
/// dédoublonné, trié, version qui progresse.
fn gerer_definir_membres(etat: &Arc<Mutex<EtatFaux>>, jeton: Option<&str>, chemin: &str, corps_brut: &str) -> (u16, String) {
    let mut e = etat.lock().expect("mutex etat faux empoisonne");
    e.appels_listes += 1;
    if let Some(r) = autoriser_appel(&mut e, jeton) {
        return r;
    }
    if e.session_partielle_totp {
        return refus(403, "Verification TOTP requise");
    }
    let Some(id) = identifiant_de_chemin(chemin, "/api/sky/lists/", "/members") else {
        return refus(400, "Identifiant invalide");
    };
    let corps: Value = match serde_json::from_str(corps_brut) {
        Ok(v @ Value::Object(_)) => v,
        _ => return refus(400, "Corps JSON invalide"),
    };
    let invalide = "membreIds invalide : attendu un tableau d'au plus 200 identifiants entiers positifs";
    let Some(bruts) = corps.get("membreIds").and_then(Value::as_array) else {
        return refus(400, invalide);
    };
    if bruts.len() > MEMBRES_LISTE_MAX {
        return refus(400, invalide);
    }
    let mut membres = Vec::with_capacity(bruts.len());
    for v in bruts {
        match v.as_i64() {
            Some(m) if m > 0 => membres.push(m),
            _ => return refus(400, invalide),
        }
    }
    if id > INTEGER_POSTGRES_MAX || membres.iter().any(|m| *m > INTEGER_POSTGRES_MAX) {
        return refus(400, REFUS_MEMBRES);
    }
    let Some(index) = e.listes.iter().position(|l| l.id == id) else {
        return refus(400, REFUS_MEMBRES);
    };
    membres.sort_unstable();
    membres.dedup();
    if !membres.iter().all(|m| e.amis.iter().any(|a| a.id == *m)) {
        return refus(400, REFUS_MEMBRES);
    }
    e.listes[index].membres = membres;
    e.version += 1;
    (204, String::new())
}
```

- [ ] **Étape 4 : relancer les tests du double**

```bash
cd spike && cargo test -p sky-compte --test faux_serveur_test
```
Attendu : PASS, dont les quatre nouveaux.

- [ ] **Étape 5 : prouver les tests du double par neutralisation (une à la fois)**

1. `nom_de_liste` : mesurer `nom.chars().count()` au lieu de `encode_utf16().count()`. Attendu :
   `le_double_refuse_un_nom_de_liste_que_le_site_refuse` rougit sur `nom_41` (21 `char` ≤ 40 →
   201). Rétablir.
2. `gerer_definir_membres` : rendre `refus(404, "Liste introuvable")` quand `index` est absent.
   Attendu : `le_double_refuse_les_membres_d_un_400_uniforme` rougit sur l'égalité des deux refus.
   Rétablir.
3. `gerer_supprimer_liste` : ajouter `e.version += 1;` avant le `204`. Attendu :
   `le_double_ne_fait_pas_progresser_la_version…` rougit. Rétablir.

- [ ] **Étape 6 : écrire les tests du client qui échouent**

Créer `spike/crates/sky-compte/tests/listes_test.rs` :

```rust
//! Listes de diffusion (jalon 1, tâche 2) contre le serveur double. Chaque
//! test nomme la ligne du site qui lit le paramètre qu'il vérifie (leçon de
//! l'essai réel du 19/09/2026 : dériver le test du serveur, pas de la
//! mémoire de ce qu'on croit devoir envoyer).
// `allow(dead_code)` : chaque binaire n'utilise qu'une partie du double.
#[allow(dead_code)]
#[path = "faux_serveur/mod.rs"]
mod faux_serveur;

use faux_serveur::{AmiFaux, FauxServeur};
use sky_compte::{
    creer_liste, definir_membres, modifier_liste, supprimer_liste, synchroniser, ChampsListe, Coffre,
    Config, CreationListe, DefinitionMembres, ErreurCompte, Jetons, Liste, ModificationListe,
    SuppressionListe,
};

fn coffre_connecte(s: &FauxServeur, prefixe: &str) -> Coffre {
    let coffre = Coffre::pour_test(prefixe);
    coffre
        .ranger_jetons(&Jetons { session: s.jeton_de_test(), renouvellement: "peu-importe".to_string() })
        .unwrap();
    coffre
}

fn creee(r: Result<CreationListe, ErreurCompte>) -> Liste {
    match r {
        Ok(CreationListe::Creee(l)) => l,
        Ok(CreationListe::NomDejaPris) => panic!("nom déjà pris inattendu"),
        Err(e) => panic!("création refusée : {e}"),
    }
}

#[test]
fn creer_liste_envoie_nom_couleur_et_emoji_sous_les_cles_que_le_site_lit() {
    // Site : `const { nom, couleur, emoji } = corps` (lists/route.ts:80).
    // Neutralisation : renommer `couleur` en `color` dans `CorpsCreation` —
    // le double lit une couleur absente, la rend nulle : l'assertion rougit.
    let s = FauxServeur::demarrer();
    let config = Config::vers(&s.url());
    let coffre = coffre_connecte(&s, "sky-test-listes-creation");

    let liste = creee(creer_liste(&config, &coffre, "Copains", Some("#a1b2c3"), Some("🎮")));
    assert!(liste.membres.is_empty(), "la création ne rend jamais de membres");

    let etat = synchroniser(&config, &coffre, None).unwrap();
    assert_eq!(etat.listes.len(), 1);
    assert_eq!(etat.listes[0].id, liste.id);
    assert_eq!(etat.listes[0].nom, "Copains");
    assert_eq!(etat.listes[0].couleur.as_deref(), Some("#a1b2c3"));
    assert_eq!(etat.listes[0].emoji.as_deref(), Some("🎮"));
}

#[test]
fn un_nom_de_41_unites_utf16_est_refuse_avant_le_reseau_et_40_passe() {
    // Site : `valeur.length <= NOM_MAX` (40), en unités UTF-16
    // (lists/route.ts:21-22). 20 émojis = 40 unités mais 20 `char` ; un « a »
    // de plus = 41 unités, 21 `char`. Neutralisation : mesurer en
    // `chars().count()` — le client laisse passer, le double répond 400,
    // `appels_listes` vaut 1 au lieu de 0.
    let s = FauxServeur::demarrer();
    let config = Config::vers(&s.url());
    let coffre = coffre_connecte(&s, "sky-test-listes-nom-41");
    let nom_40 = "😀".repeat(20);
    let nom_41 = format!("{nom_40}a");

    assert!(matches!(creer_liste(&config, &coffre, &nom_41, None, None), Err(ErreurCompte::Protocole(_))));
    assert_eq!(s.etat_mut().appels_listes, 0, "refusé avant tout appel au site");
    creee(creer_liste(&config, &coffre, &nom_40, None, None));
}

#[test]
fn un_emoji_de_9_octets_est_refuse_avant_le_reseau_et_8_passe() {
    // Site : `Buffer.byteLength(valeur, "utf8") <= EMOJI_OCTETS_MAX` (8),
    // lists/route.ts:56. « 🇫🇷 » : deux indicateurs régionaux de 4 octets.
    // Neutralisation : compter en `chars()` — « 🇫🇷a » (3 `char`) passe le
    // client, le double répond 400, `appels_listes` vaut 1.
    let s = FauxServeur::demarrer();
    let config = Config::vers(&s.url());
    let coffre = coffre_connecte(&s, "sky-test-listes-emoji");
    let drapeau = "🇫🇷";
    assert_eq!(drapeau.len(), 8);
    let neuf_octets = format!("{drapeau}a");

    assert!(matches!(
        creer_liste(&config, &coffre, "Drapeaux", None, Some(&neuf_octets)),
        Err(ErreurCompte::Protocole(_))
    ));
    assert_eq!(s.etat_mut().appels_listes, 0);
    creee(creer_liste(&config, &coffre, "Drapeaux", None, Some(drapeau)));
}

#[test]
fn une_couleur_hors_rrggbb_est_refusee_avant_le_reseau() {
    // Site : `/^#[0-9A-Fa-f]{6}$/` (lists/route.ts:37). Neutralisation :
    // n'exiger que le `#` initial — « #12345G » part, `appels_listes` > 0.
    let s = FauxServeur::demarrer();
    let config = Config::vers(&s.url());
    let coffre = coffre_connecte(&s, "sky-test-listes-couleur");
    for mauvaise in ["#12345G", "123456", "#1234567", "#12345"] {
        assert!(
            matches!(creer_liste(&config, &coffre, "C", Some(mauvaise), None), Err(ErreurCompte::Protocole(_))),
            "couleur {mauvaise}"
        );
    }
    assert_eq!(s.etat_mut().appels_listes, 0);
}

#[test]
fn un_nom_deja_pris_rend_nom_deja_pris() {
    // Site : 409 « Nom de liste deja utilise » (lists/route.ts:112-114).
    let s = FauxServeur::demarrer();
    let config = Config::vers(&s.url());
    let coffre = coffre_connecte(&s, "sky-test-listes-409");
    creee(creer_liste(&config, &coffre, "Jeu", None, None));
    assert!(matches!(creer_liste(&config, &coffre, "Jeu", None, None), Ok(CreationListe::NomDejaPris)));
}

#[test]
fn modifier_n_envoie_que_les_champs_fournis_et_null_remet_a_rien() {
    // Site : `"couleur" in donnees` (lists/[id]/route.ts:94) — clé absente =
    // inchangée, `null` = remise à rien. Neutralisation : sérialiser un champ
    // absent en `null` (`"nom": null`) — le double refuse le nom nul (400),
    // l'appel rend une erreur de protocole au lieu de `Modifiee`.
    let s = FauxServeur::demarrer();
    let config = Config::vers(&s.url());
    let coffre = coffre_connecte(&s, "sky-test-listes-modifier");
    let liste = creee(creer_liste(&config, &coffre, "Soirée", Some("#FF00AA"), Some("🎮")));

    let champs = ChampsListe { couleur: Some(None), ..ChampsListe::default() };
    assert_eq!(modifier_liste(&config, &coffre, liste.id, &champs).unwrap(), ModificationListe::Modifiee);

    let etat = synchroniser(&config, &coffre, None).unwrap();
    assert_eq!(etat.listes[0].nom, "Soirée");
    assert_eq!(etat.listes[0].couleur, None);
    assert_eq!(etat.listes[0].emoji.as_deref(), Some("🎮"));
}

#[test]
fn modifier_ou_supprimer_une_liste_inconnue_rend_introuvable() {
    // Site : 404 « Liste introuvable » (lists/[id]/route.ts:124, 158).
    let s = FauxServeur::demarrer();
    let config = Config::vers(&s.url());
    let coffre = coffre_connecte(&s, "sky-test-listes-inconnue");
    let champs = ChampsListe { nom: Some("x"), ..ChampsListe::default() };
    assert_eq!(modifier_liste(&config, &coffre, 999, &champs).unwrap(), ModificationListe::Introuvable);
    assert_eq!(supprimer_liste(&config, &coffre, 999).unwrap(), SuppressionListe::Introuvable);
}

#[test]
fn supprimer_retire_la_liste_d_une_synchronisation_complete() {
    // La suppression ne fait pas forcément progresser la version (etat.ts) :
    // seule une synchronisation SANS précédent la voit à coup sûr.
    let s = FauxServeur::demarrer();
    let config = Config::vers(&s.url());
    let coffre = coffre_connecte(&s, "sky-test-listes-supprimer");
    let a = creee(creer_liste(&config, &coffre, "A", None, None));
    let b = creee(creer_liste(&config, &coffre, "B", None, None));
    assert_eq!(supprimer_liste(&config, &coffre, a.id).unwrap(), SuppressionListe::Supprimee);

    let etat = synchroniser(&config, &coffre, None).unwrap();
    assert_eq!(etat.listes.iter().map(|l| l.id).collect::<Vec<_>>(), vec![b.id]);
}

#[test]
fn definir_membres_envoie_membre_ids_et_la_synchronisation_les_relit() {
    // Site : `const { membreIds } = corps` (members/route.ts:54).
    // Neutralisation : renommer la clé en `membres` — le double répond 400,
    // l'appel rend `Refuses`.
    let s = FauxServeur::demarrer();
    s.etat_mut().amis.push(AmiFaux::sans_appareil("bob"));
    s.etat_mut().amis.push(AmiFaux::sans_appareil("carole"));
    let (bob, carole) = { let e = s.etat_mut(); (e.amis[0].id, e.amis[1].id) };
    let config = Config::vers(&s.url());
    let coffre = coffre_connecte(&s, "sky-test-listes-membres");
    let liste = creee(creer_liste(&config, &coffre, "Équipe", None, None));

    assert_eq!(definir_membres(&config, &coffre, liste.id, &[carole, bob, bob]).unwrap(), DefinitionMembres::Definis);

    let etat = synchroniser(&config, &coffre, None).unwrap();
    let mut attendus = vec![bob, carole];
    attendus.sort_unstable();
    assert_eq!(etat.listes[0].membres, attendus);
}

#[test]
fn un_membre_qui_n_est_pas_ami_fait_refuser_sans_rien_ecrire() {
    // Site : 400 uniforme (members/route.ts:83). Rien n'est écrit.
    let s = FauxServeur::demarrer();
    s.etat_mut().amis.push(AmiFaux::sans_appareil("bob"));
    let bob = s.etat_mut().amis[0].id;
    let config = Config::vers(&s.url());
    let coffre = coffre_connecte(&s, "sky-test-listes-non-ami");
    let liste = creee(creer_liste(&config, &coffre, "L", None, None));

    assert_eq!(definir_membres(&config, &coffre, liste.id, &[bob, 424242]).unwrap(), DefinitionMembres::Refuses);
    let etat = synchroniser(&config, &coffre, None).unwrap();
    assert!(etat.listes[0].membres.is_empty());
}

#[test]
fn plus_de_200_membres_sont_refuses_avant_le_reseau() {
    // Site : `valeur.length <= MEMBRES_MAX` (members/route.ts:10).
    // Neutralisation : retirer le contrôle de longueur de `membres_valides` —
    // la requête part (`appels_listes` passe à 2) et le double répond 400.
    let s = FauxServeur::demarrer();
    let config = Config::vers(&s.url());
    let coffre = coffre_connecte(&s, "sky-test-listes-201");
    let liste = creee(creer_liste(&config, &coffre, "L", None, None));
    let trop: Vec<i64> = (1..=201).collect();

    assert!(matches!(definir_membres(&config, &coffre, liste.id, &trop), Err(ErreurCompte::Protocole(_))));
    assert_eq!(s.etat_mut().appels_listes, 1, "seule la création a atteint le site");
}
```

Dans `spike/crates/sky-compte/src/http.rs`, module `tests`, ajouter :

```rust
    #[test]
    fn aucun_jeton_dans_lechec_reseau_reel_d_un_envoi_sans_corps_de_reponse() {
        // Même preuve que pour `DELETE` : rougirait si la méthode ajoutée au
        // jalon 1 glissait `jeton` dans le message d'erreur.
        let client = ClientHttp::new(&Config::vers("http://127.0.0.1:1"));
        let r = client.envoyer_json_reponse_vide_avec_refus(
            "PATCH",
            "/api/sky/lists/1",
            &serde_json::json!({ "nom": "x" }),
            Some("SENTINEL-JETON"),
        );
        let erreur = match r {
            Err(e) => e,
            Ok(_) => panic!("attendu un échec de transport vers le port 1"),
        };
        assert!(matches!(erreur, ErreurCompte::Reseau(_)));
        assert!(!erreur.to_string().contains("SENTINEL-JETON"));
    }
```

- [ ] **Étape 7 : lancer, vérifier l'échec**

```bash
cd spike && cargo test -p sky-compte
```
Attendu : ÉCHEC à la compilation — `creer_liste`, `Liste`, `envoyer_json_reponse_vide_avec_refus`
n'existent pas, `Etat` n'a pas de champ `listes`.

- [ ] **Étape 8 : implémenter — `annuaire.rs`**

(a) Après `pub struct EnveloppeRecue { … }`, ajouter :

```rust
/// Une liste de diffusion de l'utilisateur courant, telle que rendue par
/// `GET /api/sky/sync` dans `listes[]` — forme `ListeAvecMembres` du site
/// (`src/lib/sky/listes.ts`, `listesDe`, jalon 1). `membres` : identifiants
/// d'UTILISATEURS (pas d'amitiés), triés croissants par le site.
#[derive(Debug, Clone, PartialEq)]
pub struct Liste {
    pub id: i64,
    pub nom: String,
    pub couleur: Option<String>,
    pub emoji: Option<String>,
    pub created_at: String,
    pub membres: Vec<i64>,
}
```

(b) Dans `pub struct Etat`, entre `pub demandes: Vec<Demande>,` et `pub appareils: Vec<Appareil>,` :

```rust
    pub listes: Vec<Liste>,
```

(c) Après `struct EnveloppeBrute { … }`, ajouter :

```rust
/// `membres` est EXIGÉ, sans valeur par défaut : un site qui ne le rendrait
/// pas encore (tâche 1 non déployée) ferait échouer la synchronisation en
/// erreur de protocole plutôt que d'afficher des listes faussement vides —
/// que l'écran Listes réenregistrerait vides.
#[derive(Debug, Deserialize)]
struct ListeBrute {
    id: i64,
    nom: String,
    couleur: Option<String>,
    emoji: Option<String>,
    created_at: String,
    membres: Vec<i64>,
}
```

(d) Remplacer le commentaire et le corps de `struct EtatBrut` par :

```rust
/// Forme complète de `GET /api/sky/sync`.
#[derive(Debug, Deserialize)]
struct EtatBrut {
    version: u64,
    code: String,
    amis: Vec<AmiBrut>,
    demandes: Vec<DemandeBrute>,
    listes: Vec<ListeBrute>,
    appareils: Vec<AppareilBrut>,
    enveloppes: Vec<EnveloppeBrute>,
}
```

(e) Dans `convertir_etat`, avant `let appareils = …`, ajouter :

```rust
    let listes = brut
        .listes
        .into_iter()
        .map(|l| Liste {
            id: l.id,
            nom: l.nom,
            couleur: l.couleur,
            emoji: l.emoji,
            created_at: l.created_at,
            membres: l.membres,
        })
        .collect();
```
et remplacer la dernière ligne par :
```rust
    Etat { version: brut.version, code: brut.code, amis, demandes, listes, appareils, enveloppes }
```

(f) Littéraux de test du même fichier : dans `etat_avec` (ligne ~690), dans le `EtatBrut { … }` du
test `une_cle_publique_tronquee_est_ecartee_une_valide_est_gardee` (ligne ~751) et dans le
`Etat { version: 3, … }` du test `inchange_avec_precedent_vide_les_enveloppes_et_garde_le_reste`
(ligne ~792), ajouter `listes: Vec::new(),` — rien d'autre.

- [ ] **Étape 9 : implémenter — littéraux des autres fichiers**

Ajouter `listes: Vec::new(),` (et rien d'autre) dans : `spike/crates/sky-compte/src/boite.rs:227`
(`etat_avec`), `spike/crates/sky-compte/tests/annuaire_test.rs:19` (`etat_vide`) et `:95`
(`let precedent = Etat { … }`), `spike/crates/sky-probe/src/rendez_vous.rs:291` (fonction `etat`
des tests).

- [ ] **Étape 10 : implémenter — `http.rs`**

Dans `impl ClientHttp`, après `delete_reponse_vide_avec_refus` :

```rust
    /// Requête à méthode explicite (`PATCH`, `PUT`) avec un corps JSON, pour une
    /// route dont le SUCCÈS ne porte aucun corps (`204`) — AJOUTÉE AU JALON 1
    /// pour les listes (`PATCH /api/sky/lists/{id}`, `PUT
    /// /api/sky/lists/{id}/members`). Mêmes garanties que
    /// `post_json_reponse_vide_avec_refus` : délai de l'agent, succès jamais
    /// décodé, 401 → `Err(Refuse)` pour que `avec_jeton_valide` renouvelle,
    /// transport → `Err(Reseau)`, autre statut → `Ok(Refus)` au corps rédigé.
    pub fn envoyer_json_reponse_vide_avec_refus<B: Serialize>(
        &self,
        methode: &str,
        chemin: &str,
        corps: &B,
        jeton: Option<&str>,
    ) -> Result<ReponseHttp<()>, ErreurCompte> {
        let requete = Self::avec_jeton(self.agent.request(methode, &self.url(chemin)), jeton);
        match Self::issue_requete(requete.send_json(corps))? {
            IssueRequete::Succes(_reponse_2xx) => Ok(ReponseHttp::Succes(())),
            IssueRequete::Refus { statut, corps } => Ok(ReponseHttp::Refus { statut, corps }),
        }
    }
```

- [ ] **Étape 11 : implémenter — `listes.rs`**

Créer `spike/crates/sky-compte/src/listes.rs` :

```rust
//! Listes de diffusion (jalon 1, spec §3) : création, modification,
//! suppression, définition des membres. La LECTURE passe par `synchroniser`
//! (`Etat::listes`, membres compris) : aucune route de lecture ici.
//!
//! Chaque borne est celle de la route du site qui lit le paramètre
//! (`src/app/api/sky/lists/route.ts`, `lists/[id]/route.ts`,
//! `lists/[id]/members/route.ts`, `src/lib/sky/listes.ts`), appliquée AVANT
//! tout appel réseau : on valide contre ce que le serveur refuse, ni plus
//! strictement, ni plus largement.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::annuaire::Liste;
use crate::erreur::ErreurCompte;
use crate::http::{ClientHttp, Config, ReponseHttp};
use crate::session::avec_jeton_valide;
use crate::Coffre;

/// `NOM_MAX` de `lists/route.ts` et `lists/[id]/route.ts`.
const NOM_MAX: usize = 40;
/// `EMOJI_OCTETS_MAX` des mêmes routes.
const EMOJI_OCTETS_MAX: usize = 8;
/// `MEMBRES_MAX` de `members/route.ts` et de `definirMembres`.
pub const MEMBRES_MAX: usize = 200;
/// `POSTGRES_INTEGER_MAX` de `listes.ts` : au-delà, `definirMembres` refuse.
const POSTGRES_INTEGER_MAX: i64 = 2_147_483_647;

/// `nomValide` : 1 à 40 unités UTF-16 (`length` en JavaScript, pas des
/// `char`), sans octet NUL (`texteStockable`). Un substitut isolé ne peut
/// pas exister dans un `&str`.
pub fn nom_liste_valide(nom: &str) -> bool {
    let unites = nom.encode_utf16().count();
    (1..=NOM_MAX).contains(&unites) && !nom.contains('\0')
}

/// `couleurValide` : aucune, ou exactement `#` suivi de six chiffres
/// hexadécimaux, majuscules ou minuscules.
pub fn couleur_valide(couleur: Option<&str>) -> bool {
    match couleur {
        None => true,
        Some(c) => c.len() == 7 && c.starts_with('#') && c[1..].bytes().all(|o| o.is_ascii_hexdigit()),
    }
}

/// `emojiValide` : aucun, ou 8 OCTETS UTF-8 au plus (`Buffer.byteLength`),
/// sans octet NUL. Une chaîne vide est acceptée par le site : elle l'est ici.
pub fn emoji_valide(emoji: Option<&str>) -> bool {
    match emoji {
        None => true,
        Some(e) => e.len() <= EMOJI_OCTETS_MAX && !e.contains('\0'),
    }
}

/// `membreIdsValide` puis les gardes de `definirMembres` : au plus 200
/// identifiants, chacun entre 1 et la borne INTEGER de Postgres.
pub fn membres_valides(membres: &[i64]) -> bool {
    membres.len() <= MEMBRES_MAX && membres.iter().all(|m| (1..=POSTGRES_INTEGER_MAX).contains(m))
}

fn refuse_avant_le_reseau(motif: &str) -> ErreurCompte {
    ErreurCompte::Protocole(format!("{motif} — refusé avant tout appel au site"))
}

fn statut_inattendu(statut: u16, corps: String) -> ErreurCompte {
    ErreurCompte::Protocole(format!("statut {statut} inattendu : {corps}"))
}

/// Issue d'une création : le nom déjà pris est un refus ATTENDU, pas une
/// erreur (409, `lists/route.ts:112`).
#[derive(Debug, Clone, PartialEq)]
pub enum CreationListe {
    Creee(Liste),
    NomDejaPris,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModificationListe {
    Modifiee,
    /// 404 : inexistante OU pas à l'appelant — indiscernables à dessein.
    Introuvable,
    NomDejaPris,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuppressionListe {
    Supprimee,
    Introuvable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefinitionMembres {
    Definis,
    /// 400 UNIFORME du site : liste pas à l'appelant, OU un identifiant qui
    /// n'est pas un ami accepté. Le client ne cherche pas à les distinguer.
    Refuses,
}

/// Champs d'une modification partielle. `None` : clé absente, champ
/// inchangé. `Some(None)` pour `couleur`/`emoji` : `null`, remise à rien.
#[derive(Debug, Clone, Copy, Default)]
pub struct ChampsListe<'a> {
    pub nom: Option<&'a str>,
    pub couleur: Option<Option<&'a str>>,
    pub emoji: Option<Option<&'a str>>,
}

impl ChampsListe<'_> {
    /// Corps JSON du `PATCH` : SEULES les clés présentes, comme `"cle" in
    /// donnees` côté site.
    fn en_corps(&self) -> Map<String, Value> {
        let mut corps = Map::new();
        if let Some(nom) = self.nom {
            corps.insert("nom".to_string(), Value::from(nom));
        }
        if let Some(couleur) = self.couleur {
            corps.insert("couleur".to_string(), couleur.map_or(Value::Null, Value::from));
        }
        if let Some(emoji) = self.emoji {
            corps.insert("emoji".to_string(), emoji.map_or(Value::Null, Value::from));
        }
        corps
    }
}

#[derive(Serialize)]
struct CorpsCreation<'a> {
    nom: &'a str,
    couleur: Option<&'a str>,
    emoji: Option<&'a str>,
}

/// Réponse 201 de `POST /api/sky/lists` : `COLONNES_LISTE`, sans `membres`.
#[derive(Deserialize)]
struct ListeCreee {
    id: i64,
    nom: String,
    couleur: Option<String>,
    emoji: Option<String>,
    created_at: String,
}

#[derive(Serialize)]
struct CorpsMembres<'a> {
    #[serde(rename = "membreIds")]
    membre_ids: &'a [i64],
}

/// `POST /api/sky/lists`.
pub fn creer_liste(
    config: &Config,
    coffre: &Coffre,
    nom: &str,
    couleur: Option<&str>,
    emoji: Option<&str>,
) -> Result<CreationListe, ErreurCompte> {
    if !nom_liste_valide(nom) {
        return Err(refuse_avant_le_reseau("nom de liste invalide : 1 à 40 unités UTF-16, sans octet NUL"));
    }
    if !couleur_valide(couleur) {
        return Err(refuse_avant_le_reseau("couleur invalide : #RRGGBB ou aucune"));
    }
    if !emoji_valide(emoji) {
        return Err(refuse_avant_le_reseau("émoji invalide : 8 octets au plus"));
    }
    let client = ClientHttp::new(config);
    let demande = CorpsCreation { nom, couleur, emoji };
    avec_jeton_valide(config, coffre, |jeton| {
        let issue: ReponseHttp<ListeCreee> = client.post_json_avec_refus("/api/sky/lists", &demande, Some(jeton))?;
        match issue {
            ReponseHttp::Succes(l) => Ok(CreationListe::Creee(Liste {
                id: l.id,
                nom: l.nom,
                couleur: l.couleur,
                emoji: l.emoji,
                created_at: l.created_at,
                membres: Vec::new(),
            })),
            ReponseHttp::Refus { statut: 409, .. } => Ok(CreationListe::NomDejaPris),
            ReponseHttp::Refus { statut, corps } => Err(statut_inattendu(statut, corps)),
        }
    })
}

/// `PATCH /api/sky/lists/{id}`.
pub fn modifier_liste(
    config: &Config,
    coffre: &Coffre,
    id: i64,
    champs: &ChampsListe<'_>,
) -> Result<ModificationListe, ErreurCompte> {
    if id <= 0 {
        return Err(refuse_avant_le_reseau("identifiant de liste invalide"));
    }
    if champs.nom.is_none() && champs.couleur.is_none() && champs.emoji.is_none() {
        return Err(refuse_avant_le_reseau("aucun champ à modifier"));
    }
    if champs.nom.is_some_and(|n| !nom_liste_valide(n)) {
        return Err(refuse_avant_le_reseau("nom de liste invalide : 1 à 40 unités UTF-16, sans octet NUL"));
    }
    if champs.couleur.is_some_and(|c| !couleur_valide(c)) {
        return Err(refuse_avant_le_reseau("couleur invalide : #RRGGBB ou aucune"));
    }
    if champs.emoji.is_some_and(|e| !emoji_valide(e)) {
        return Err(refuse_avant_le_reseau("émoji invalide : 8 octets au plus"));
    }
    let client = ClientHttp::new(config);
    let chemin = format!("/api/sky/lists/{id}");
    let demande = champs.en_corps();
    avec_jeton_valide(config, coffre, |jeton| {
        match client.envoyer_json_reponse_vide_avec_refus("PATCH", &chemin, &demande, Some(jeton))? {
            ReponseHttp::Succes(()) => Ok(ModificationListe::Modifiee),
            ReponseHttp::Refus { statut: 404, .. } => Ok(ModificationListe::Introuvable),
            ReponseHttp::Refus { statut: 409, .. } => Ok(ModificationListe::NomDejaPris),
            ReponseHttp::Refus { statut, corps } => Err(statut_inattendu(statut, corps)),
        }
    })
}

/// `DELETE /api/sky/lists/{id}`. Attention : la version de synchronisation
/// peut ne pas progresser (la ligne disparaît) — l'appelant qui veut voir la
/// suppression resynchronise SANS précédent.
pub fn supprimer_liste(config: &Config, coffre: &Coffre, id: i64) -> Result<SuppressionListe, ErreurCompte> {
    if id <= 0 {
        return Err(refuse_avant_le_reseau("identifiant de liste invalide"));
    }
    let client = ClientHttp::new(config);
    let chemin = format!("/api/sky/lists/{id}");
    avec_jeton_valide(config, coffre, |jeton| match client.delete_reponse_vide_avec_refus(&chemin, Some(jeton))? {
        ReponseHttp::Succes(()) => Ok(SuppressionListe::Supprimee),
        ReponseHttp::Refus { statut: 404, .. } => Ok(SuppressionListe::Introuvable),
        ReponseHttp::Refus { statut, corps } => Err(statut_inattendu(statut, corps)),
    })
}

/// `PUT /api/sky/lists/{id}/members` — remplace TOUT l'ensemble des membres.
///
/// Après les contrôles faits ici (identifiant, taille, bornes), un 400 du
/// site ne peut plus être qu'un refus de forme déjà exclu ou le refus
/// uniforme de `definirMembres` : tout 400 devient donc `Refuses`.
pub fn definir_membres(
    config: &Config,
    coffre: &Coffre,
    id: i64,
    membres: &[i64],
) -> Result<DefinitionMembres, ErreurCompte> {
    if id <= 0 {
        return Err(refuse_avant_le_reseau("identifiant de liste invalide"));
    }
    if !membres_valides(membres) {
        return Err(refuse_avant_le_reseau("membres invalides : 200 identifiants au plus, chacun positif"));
    }
    let client = ClientHttp::new(config);
    let chemin = format!("/api/sky/lists/{id}/members");
    let demande = CorpsMembres { membre_ids: membres };
    avec_jeton_valide(config, coffre, |jeton| {
        match client.envoyer_json_reponse_vide_avec_refus("PUT", &chemin, &demande, Some(jeton))? {
            ReponseHttp::Succes(()) => Ok(DefinitionMembres::Definis),
            ReponseHttp::Refus { statut: 400, .. } => Ok(DefinitionMembres::Refuses),
            ReponseHttp::Refus { statut, corps } => Err(statut_inattendu(statut, corps)),
        }
    })
}
```

Dans `spike/crates/sky-compte/src/lib.rs` : ajouter `pub mod listes;` après `pub mod identite;`,
ajouter `Liste,` à la liste `pub use annuaire::{ … }` (ordre alphabétique : après `Etat,`), et
ajouter la ligne :

```rust
pub use listes::{
    creer_liste, definir_membres, modifier_liste, supprimer_liste, ChampsListe, CreationListe,
    DefinitionMembres, ModificationListe, SuppressionListe,
};
```

- [ ] **Étape 12 : lancer les tests**

```bash
cd spike && cargo test -p sky-compte && cargo test -p sky-probe && cargo clippy --all-targets -- -D warnings
```
Attendu : PASS partout ; les onze tests de `listes_test.rs` et celui de `http.rs` verts ; le nombre
de tests de `sky-probe` inchangé.

- [ ] **Étape 13 : prouver les tests du client par neutralisation (une à la fois)**

Appliquer, **une par une**, les neutralisations écrites en tête de chaque test de
`listes_test.rs` (`color` au lieu de `couleur` ; `chars().count()` pour le nom ; `chars()` pour
l'émoji ; `#` seul pour la couleur ; `"nom": null` pour une clé absente ; `membres` au lieu de
`membreIds` ; plus de contrôle de longueur des membres). Pour chacune, noter le test qui rougit et le
message, vérifier qu'aucun autre test ne rougit, rétablir. Pour `http.rs` : remplacer dans
`issue_requete` le message de transport par `format!("{transport} {jeton:?}")` n'est pas possible
(`issue_requete` ne voit pas le jeton) — neutraliser en faisant rendre à
`envoyer_json_reponse_vide_avec_refus` `Err(ErreurCompte::Reseau(format!("{:?}", jeton)))` avant
l'envoi ; le test rougit. Rétablir.

- [ ] **Étape 14 : commiter et pousser**

```bash
git add spike/crates/sky-compte/src/annuaire.rs
git add spike/crates/sky-compte/src/listes.rs
git add spike/crates/sky-compte/src/http.rs
git add spike/crates/sky-compte/src/lib.rs
git add spike/crates/sky-compte/src/boite.rs
git add spike/crates/sky-compte/tests/annuaire_test.rs
git add spike/crates/sky-compte/tests/listes_test.rs
git add spike/crates/sky-compte/tests/faux_serveur/mod.rs
git add spike/crates/sky-compte/tests/faux_serveur/tests_du_double.rs
git add spike/crates/sky-probe/src/rendez_vous.rs
git diff --cached --stat
git commit -m "feat: sky-compte lit les listes et leurs membres, les cree, modifie, supprime"
git push -u origin jalon-1-application
```
Lire la ligne `a..b` (ou `* [new branch]`) de la sortie de `git push`.

---

## Task 3 : `sky-compte` — amis et compte

**Ce qui existe déjà (relevé, pas supposé).** `revoquer_appareil` existe dans `annuaire.rs:475`
(C2, vague finale I1), **privée**, utilisée par `rattacher_appareil` : `DELETE
/api/sky/devices/{id}`, 204 et 404 acceptés. Elle devient `pub`. `nom_appareil_valide`
(`annuaire.rs:341`) devient `pub` : l'application en a besoin pour le nom par défaut (spec §10).
`Ami` ne porte **pas** `friendshipId` alors que le site l'envoie (`amisDe`, `amis.ts:541`) : or les
trois routes `friends/{id}` lisent un identifiant d'**amitié**. Il faut l'ajouter.

**Les bornes, relevées dans le site :**

| Paramètre | Ligne du site qui le lit | Règle |
|---|---|---|
| `{id}` de `DELETE /api/sky/friends/{id}` | `friends/[id]/route.ts:17-22` | `Number(id)`, entier > 0 sinon 400 ; `retirerAmi` : 404 « Amitié introuvable » sinon 204 |
| `{id}` de `POST /api/sky/friends/{id}/block` | `friends/[id]/block/route.ts:16-27` | même forme ; 200 `{ ok: true }` ou 404 |
| `POST /api/sky/friend-code` | `friend-code/route.ts:32-42` | aucun corps lu ; 200 `{ code }` |
| `friendshipId` de la synchronisation | `amis.ts:541` (`f.id AS "friendshipId"`) | entier, camelCase |

**Deux comportements du site que le double reproduit exprès.** `retirerAmi` **supprime** la ligne
`friendships` (`amis.ts:291-296`) : la version peut ne pas progresser, un client qui synchronise avec
`?version=` peut ne jamais voir le retrait — le double ne fait pas progresser `version`.
`regenererCode` écrit `users.friend_code`, **absent** du calcul de version (`etat.ts:200-209`) :
même chose. `bloquerAmi` pose `updated_at = NOW()` : la version progresse.

> **Défaut du site constaté en relevant ces lignes, hors périmètre de ce jalon :** l'ami **retiré**,
> lui, garde l'ami qui l'a retiré dans sa liste tant que rien d'autre ne fait progresser sa propre
> version. L'application le contourne pour les gestes faits sur la machine (resynchronisation
> complète après chaque commande, T7), pas pour ceux de l'autre partie. Signalé au propriétaire ;
> ne pas le corriger ici (spec §5 : une seule modification du site).

**Files:**
- Modify: `spike/crates/sky-compte/src/annuaire.rs` (`Ami`, `AmiBrut`, `convertir_etat`, nouvelles fonctions, `pub` sur deux fonctions, fixture de test ligne 675)
- Modify: `spike/crates/sky-compte/src/lib.rs`
- Modify: `spike/crates/sky-compte/tests/annuaire_test.rs:98` (littéral `Ami`)
- Modify: `spike/crates/sky-probe/src/rendez_vous.rs` (six littéraux `Ami { … }` des tests : lignes 351, 359, 393, 409, 440, 685)
- Modify: `spike/crates/sky-compte/tests/faux_serveur/mod.rs`
- Create: `spike/crates/sky-compte/tests/amis_test.rs`

**Interfaces:**
- Consumes : `ClientHttp::{delete_reponse_vide_avec_refus, post_json_avec_refus, post_json}`,
  `avec_jeton_valide` ; `identifiant_de_chemin` du double (T2).
- Produces :
  - `Ami` gagne `pub friendship_id: i64` (deuxième champ, après `id`)
  - `pub enum Retrait { Retire, Introuvable }`, `pub enum Blocage { Bloque, Introuvable }` (`Debug, Clone, Copy, PartialEq, Eq`)
  - `pub fn retirer_ami(config: &Config, coffre: &Coffre, friendship_id: i64) -> Result<Retrait, ErreurCompte>`
  - `pub fn bloquer_ami(config: &Config, coffre: &Coffre, friendship_id: i64) -> Result<Blocage, ErreurCompte>`
  - `pub fn regenerer_code(config: &Config, coffre: &Coffre) -> Result<String, ErreurCompte>`
  - `pub fn revoquer_appareil(config: &Config, coffre: &Coffre, id: i64) -> Result<(), ErreurCompte>` (existante, rendue publique)
  - `pub fn nom_appareil_valide(nom: &str) -> bool` (existante, rendue publique)
  - Tous réexportés à la racine de `sky_compte`.
  - Serveur double : champs `EtatFaux::{code_ami: Option<String>, codes_regeneres: u64, appels_amis: u64}`.

- [ ] **Étape 1 : écrire les tests qui échouent**

Créer `spike/crates/sky-compte/tests/amis_test.rs` :

```rust
//! Amis et compte (jalon 1, tâche 3) contre le serveur double : retirer,
//! bloquer, régénérer le code, révoquer un appareil.
#[allow(dead_code)]
#[path = "faux_serveur/mod.rs"]
mod faux_serveur;

use faux_serveur::{AmiFaux, FauxServeur};
use sky_compte::{
    bloquer_ami, enregistrer_appareil, regenerer_code, retirer_ami, revoquer_appareil, synchroniser,
    Blocage, Coffre, Config, ErreurCompte, Jetons, Retrait,
};

fn coffre_connecte(s: &FauxServeur, prefixe: &str) -> Coffre {
    let coffre = Coffre::pour_test(prefixe);
    coffre
        .ranger_jetons(&Jetons { session: s.jeton_de_test(), renouvellement: "peu-importe".to_string() })
        .unwrap();
    coffre
}

/// Un ami dont l'identifiant d'AMITIÉ diffère de son identifiant
/// d'utilisateur : `AmiFaux::sans_appareil` leur donne la même valeur, ce qui
/// rendrait invisible une confusion entre les deux.
fn ami_d_amitie(nom: &str, friendship_id: i64) -> AmiFaux {
    let mut ami = AmiFaux::sans_appareil(nom);
    ami.friendship_id = friendship_id;
    ami
}

#[test]
fn la_synchronisation_lit_friendship_id_distinct_de_l_identifiant() {
    // Site : `f.id AS "friendshipId"` (amisDe, amis.ts:541). Neutralisation :
    // `#[serde(rename = "id")]` sur `friendship_id` d'`AmiBrut` — rougit.
    let s = FauxServeur::demarrer();
    s.etat_mut().amis.push(ami_d_amitie("bob", 777));
    let coffre = coffre_connecte(&s, "sky-test-amis-friendship-id");
    let etat = synchroniser(&Config::vers(&s.url()), &coffre, None).unwrap();
    assert_eq!(etat.amis[0].friendship_id, 777);
    assert_ne!(etat.amis[0].id, 777);
}

#[test]
fn retirer_ami_vise_l_identifiant_d_amitie_que_la_route_lit() {
    // Site : `const friendshipId = Number(id)` (friends/[id]/route.ts:17).
    // Neutralisation : construire le chemin avec 1 + friendship_id — le
    // double rend 404, l'appel rend `Introuvable`.
    let s = FauxServeur::demarrer();
    s.etat_mut().amis.push(ami_d_amitie("bob", 777));
    let config = Config::vers(&s.url());
    let coffre = coffre_connecte(&s, "sky-test-amis-retirer");

    assert_eq!(retirer_ami(&config, &coffre, 777).unwrap(), Retrait::Retire);
    assert!(synchroniser(&config, &coffre, None).unwrap().amis.is_empty());
    assert_eq!(retirer_ami(&config, &coffre, 777).unwrap(), Retrait::Introuvable);
}

#[test]
fn un_identifiant_d_amitie_nul_ou_negatif_est_refuse_avant_le_reseau() {
    // Site : `!Number.isInteger(friendshipId) || friendshipId <= 0` → 400
    // (friends/[id]/route.ts:18). Neutralisation : retirer la garde — les
    // requêtes partent, `appels_amis` vaut 2.
    let s = FauxServeur::demarrer();
    let config = Config::vers(&s.url());
    let coffre = coffre_connecte(&s, "sky-test-amis-identifiant");
    assert!(matches!(retirer_ami(&config, &coffre, 0), Err(ErreurCompte::Protocole(_))));
    assert!(matches!(bloquer_ami(&config, &coffre, -3), Err(ErreurCompte::Protocole(_))));
    assert_eq!(s.etat_mut().appels_amis, 0);
}

#[test]
fn bloquer_ami_poste_sur_block_et_retire_l_ami() {
    // Site : `POST /api/sky/friends/{id}/block` → 200 `{ ok: true }`
    // (block/route.ts:29). Neutralisation : poster sur `/accept` — le double
    // rend 404 (amitié non acceptable), l'appel rend `Introuvable`.
    let s = FauxServeur::demarrer();
    s.etat_mut().amis.push(ami_d_amitie("bob", 778));
    let config = Config::vers(&s.url());
    let coffre = coffre_connecte(&s, "sky-test-amis-bloquer");
    assert_eq!(bloquer_ami(&config, &coffre, 778).unwrap(), Blocage::Bloque);
    assert!(synchroniser(&config, &coffre, None).unwrap().amis.is_empty());
    assert_eq!(bloquer_ami(&config, &coffre, 778).unwrap(), Blocage::Introuvable);
}

#[test]
fn regenerer_code_rend_le_code_que_la_synchronisation_complete_relit() {
    // Site : `return NextResponse.json({ code })` (friend-code/route.ts:42).
    // Neutralisation : lire un champ `nouveau_code` — erreur de protocole.
    let s = FauxServeur::demarrer();
    let config = Config::vers(&s.url());
    let coffre = coffre_connecte(&s, "sky-test-amis-code");
    let avant = synchroniser(&config, &coffre, None).unwrap().code;
    let code = regenerer_code(&config, &coffre).unwrap();
    assert_ne!(code, avant);
    assert_eq!(synchroniser(&config, &coffre, None).unwrap().code, code);
}

#[test]
fn revoquer_appareil_revoque_un_autre_appareil_et_accepte_un_inconnu() {
    // Deux enregistrements sur la même session : le second détache le
    // premier (`enregistrerAppareil`), dont la révocation ne coupe donc pas
    // la session du test.
    let s = FauxServeur::demarrer();
    let config = Config::vers(&s.url());
    let coffre = coffre_connecte(&s, "sky-test-amis-revoquer");
    let ancien = enregistrer_appareil(&config, &coffre, "Ancien PC", &[3u8; 32]).unwrap();
    let _courant = enregistrer_appareil(&config, &coffre, "PC", &[4u8; 32]).unwrap();

    revoquer_appareil(&config, &coffre, ancien).unwrap();
    let etat = synchroniser(&config, &coffre, None).unwrap();
    let revoque = etat.appareils.iter().find(|a| a.id == ancien).unwrap();
    assert!(revoque.revoked_at.is_some());
    revoquer_appareil(&config, &coffre, 999).unwrap();
}
```

- [ ] **Étape 2 : lancer, vérifier l'échec**

```bash
cd spike && cargo test -p sky-compte --test amis_test
```
Attendu : ÉCHEC à la compilation — `retirer_ami`, `Retrait`, `friendship_id` n'existent pas,
`revoquer_appareil` est privée.

- [ ] **Étape 3 : étendre le double**

Dans `spike/crates/sky-compte/tests/faux_serveur/mod.rs` :

(a) À la fin de `pub struct EtatFaux` :

```rust
    /// Code ami rendu par `GET /api/sky/sync` ; `None` : `CODE_FAUX`.
    /// Remplacé par `POST /api/sky/friend-code` (jalon 1, tâche 3).
    pub code_ami: Option<String>,
    /// Nombre de régénérations du code ami.
    pub codes_regeneres: u64,
    /// Requêtes reçues sur `/api/sky/friends*` (ajout, acceptation, retrait,
    /// blocage), refusées comprises.
    pub appels_amis: u64,
```

(b) Dans `gerer_sync`, remplacer `"code": CODE_FAUX,` par :

```rust
        "code": e.code_ami.clone().unwrap_or_else(|| CODE_FAUX.to_string()),
```

(c) Dans `gerer_friends` et dans `gerer_accepter_ami`, juste après la ligne
`let mut e = etat.lock().expect("mutex etat faux empoisonne");`, ajouter :

```rust
    e.appels_amis += 1;
```

(d) Dans le `match` de `repondre`, avant `_ => (404u16, …` :

```rust
        (Method::Post, "/api/sky/friend-code") => gerer_regenerer_code(etat, jeton.as_deref()),
        (Method::Post, chemin_bloc)
            if chemin_bloc.starts_with("/api/sky/friends/") && chemin_bloc.ends_with("/block") =>
        {
            gerer_bloquer_ami(etat, jeton.as_deref(), chemin_bloc)
        }
        (Method::Delete, chemin_ami) if chemin_ami.starts_with("/api/sky/friends/") => {
            gerer_retirer_ami(etat, jeton.as_deref(), chemin_ami)
        }
```

(e) À la fin du fichier (avant le commentaire final) :

```rust
// --- Amis et compte (jalon 1, tâche 3) ------------------------------------

/// `DELETE /api/sky/friends/{id}` — `retirerAmi` (amis.ts:288-300) SUPPRIME la
/// ligne `friendships`, amitié acceptée OU demande en attente : 204, sinon 404
/// « Amitié introuvable ». La version NE progresse PAS : le site peut rendre
/// `inchange` après un retrait (voir le plan du jalon 1, tâche 3).
fn gerer_retirer_ami(etat: &Arc<Mutex<EtatFaux>>, jeton: Option<&str>, chemin: &str) -> (u16, String) {
    let mut e = etat.lock().expect("mutex etat faux empoisonne");
    e.appels_amis += 1;
    if let Some(r) = autoriser_appel(&mut e, jeton) {
        return r;
    }
    if e.session_partielle_totp {
        return refus(403, "Verification TOTP requise");
    }
    let Some(id) = identifiant_de_chemin(chemin, "/api/sky/friends/", "") else {
        return refus(400, "Identifiant invalide");
    };
    let avant = e.amis.len() + e.demandes.len();
    e.amis.retain(|a| a.friendship_id != id);
    e.demandes.retain(|d| d.friendship_id != id);
    if e.amis.len() + e.demandes.len() == avant {
        return refus(404, "Amitié introuvable");
    }
    (204, String::new())
}

/// `POST /api/sky/friends/{id}/block` — `bloquerAmi` (amis.ts:345-358) : la
/// ligne passe en `bloquee` (elle quitte `amisDe` et `demandesDe`) avec
/// `updated_at = NOW()` : la version progresse. 200 `{ ok: true }` ou 404.
fn gerer_bloquer_ami(etat: &Arc<Mutex<EtatFaux>>, jeton: Option<&str>, chemin: &str) -> (u16, String) {
    let mut e = etat.lock().expect("mutex etat faux empoisonne");
    e.appels_amis += 1;
    if let Some(r) = autoriser_appel(&mut e, jeton) {
        return r;
    }
    if e.session_partielle_totp {
        return refus(403, "Verification TOTP requise");
    }
    let Some(id) = identifiant_de_chemin(chemin, "/api/sky/friends/", "/block") else {
        return refus(400, "Identifiant invalide");
    };
    let avant = e.amis.len() + e.demandes.len();
    e.amis.retain(|a| a.friendship_id != id);
    e.demandes.retain(|d| d.friendship_id != id);
    if e.amis.len() + e.demandes.len() == avant {
        return refus(404, "Amitié introuvable");
    }
    e.version += 1;
    (200, json!({ "ok": true }).to_string())
}

/// `POST /api/sky/friend-code` — `regenererCode` (codes.ts:81) : 200
/// `{ code }`. Écrit `users.friend_code`, absent du calcul de version : la
/// version NE progresse PAS. Codes tirés dans l'alphabet réel.
fn gerer_regenerer_code(etat: &Arc<Mutex<EtatFaux>>, jeton: Option<&str>) -> (u16, String) {
    const CODES: [&str; 3] = ["REGENAA2", "REGENBB3", "REGENCC4"];
    let mut e = etat.lock().expect("mutex etat faux empoisonne");
    if let Some(r) = autoriser_appel(&mut e, jeton) {
        return r;
    }
    if e.session_partielle_totp {
        return refus(403, "Verification TOTP requise");
    }
    let code = CODES[(e.codes_regeneres % 3) as usize];
    e.codes_regeneres += 1;
    e.code_ami = Some(code.to_string());
    (200, json!({ "code": code }).to_string())
}
```

- [ ] **Étape 4 : implémenter — `annuaire.rs`**

(a) `pub struct Ami` devient :

```rust
pub struct Ami {
    pub id: i64,
    /// Identifiant de l'AMITIÉ (`friendshipId`), celui que lisent les routes
    /// `friends/{id}` — distinct de `id`, qui désigne l'utilisateur.
    pub friendship_id: i64,
    pub discord_name: String,
    pub appareils: Vec<AppareilDAmi>,
}
```

(b) `struct AmiBrut` gagne, après `id: i64,` :

```rust
    #[serde(rename = "friendshipId")]
    friendship_id: i64,
```

(c) Dans `convertir_etat`, le `.map(|a| Ami { … })` gagne `friendship_id: a.friendship_id,`.

(d) `fn nom_appareil_valide` devient `pub fn nom_appareil_valide` ; `fn revoquer_appareil` devient
`pub fn revoquer_appareil` (docs inchangées, plus une phrase : « Publique depuis le jalon 1 :
l'écran Mon compte révoque un autre appareil par elle. »).

(e) Après `accepter_ami`, ajouter :

```rust
/// Borne des routes `friends/{id}` : `Number.isInteger(x) && x > 0`, sinon 400
/// (`friends/[id]/route.ts:18`, `block/route.ts:18`). Refusé ici avant tout
/// réseau.
fn identifiant_d_amitie_valide(friendship_id: i64) -> bool {
    friendship_id > 0
}

/// Issue d'un `retirer_ami`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retrait {
    /// 204.
    Retire,
    /// 404 « Amitié introuvable » : inexistante, d'autrui, ou bloquée par
    /// l'autre partie — indiscernables à dessein côté site.
    Introuvable,
}

/// `DELETE /api/sky/friends/{friendship_id}` — retire un ami ou une demande.
///
/// Le site SUPPRIME la ligne : la version de synchronisation peut ne pas
/// progresser. Pour voir le retrait, resynchroniser SANS précédent.
pub fn retirer_ami(config: &Config, coffre: &Coffre, friendship_id: i64) -> Result<Retrait, ErreurCompte> {
    if !identifiant_d_amitie_valide(friendship_id) {
        return Err(ErreurCompte::Protocole(format!("identifiant d'amitié invalide : {friendship_id}")));
    }
    let client = ClientHttp::new(config);
    let chemin = format!("/api/sky/friends/{friendship_id}");
    avec_jeton_valide(config, coffre, |jeton| match client.delete_reponse_vide_avec_refus(&chemin, Some(jeton))? {
        ReponseHttp::Succes(()) => Ok(Retrait::Retire),
        ReponseHttp::Refus { statut: 404, .. } => Ok(Retrait::Introuvable),
        ReponseHttp::Refus { statut, corps } => {
            Err(ErreurCompte::Protocole(format!("statut {statut} inattendu : {corps}")))
        }
    })
}

/// Issue d'un `bloquer_ami`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocage {
    /// 200 `{ ok: true }`.
    Bloque,
    /// 404, même recouvrement que `Retrait::Introuvable`.
    Introuvable,
}

/// `POST /api/sky/friends/{friendship_id}/block` — la route ne lit aucun
/// corps : un objet vide est envoyé.
pub fn bloquer_ami(config: &Config, coffre: &Coffre, friendship_id: i64) -> Result<Blocage, ErreurCompte> {
    if !identifiant_d_amitie_valide(friendship_id) {
        return Err(ErreurCompte::Protocole(format!("identifiant d'amitié invalide : {friendship_id}")));
    }
    let client = ClientHttp::new(config);
    let chemin = format!("/api/sky/friends/{friendship_id}/block");
    let corps_vide = serde_json::json!({});
    avec_jeton_valide(config, coffre, |jeton| {
        let issue: ReponseHttp<ReponseAcceptation> = client.post_json_avec_refus(&chemin, &corps_vide, Some(jeton))?;
        match issue {
            ReponseHttp::Succes(_) => Ok(Blocage::Bloque),
            ReponseHttp::Refus { statut: 404, .. } => Ok(Blocage::Introuvable),
            ReponseHttp::Refus { statut, corps } => {
                Err(ErreurCompte::Protocole(format!("statut {statut} inattendu : {corps}")))
            }
        }
    })
}

#[derive(Deserialize)]
struct ReponseCode {
    code: String,
}

/// `POST /api/sky/friend-code` — tire un nouveau code ami et le rend.
/// L'ancien cesse aussitôt de fonctionner. Le code n'entre pas dans la
/// version de synchronisation : pour le relire, resynchroniser SANS précédent.
pub fn regenerer_code(config: &Config, coffre: &Coffre) -> Result<String, ErreurCompte> {
    let client = ClientHttp::new(config);
    let corps_vide = serde_json::json!({});
    let reponse: ReponseCode =
        avec_jeton_valide(config, coffre, |jeton| client.post_json("/api/sky/friend-code", &corps_vide, Some(jeton)))?;
    Ok(reponse.code)
}
```

(f) Fixture de test ligne 675 : `Ami { id, friendship_id: id, discord_name: nom.to_string(), appareils }`.

- [ ] **Étape 5 : littéraux des autres fichiers**

Ajouter `friendship_id: <valeur de id>,` (et rien d'autre) à chaque littéral `Ami { … }` :
`spike/crates/sky-compte/tests/annuaire_test.rs:98` (`friendship_id: 1`) et les six de
`spike/crates/sky-probe/src/rendez_vous.rs` (lignes 351, 359, 393, 409, 440, 685 ; valeurs 1, 2, 1,
1, 1, 5 — celles du champ `id` du même littéral).

- [ ] **Étape 6 : réexporter**

Dans `spike/crates/sky-compte/src/lib.rs`, la liste `pub use annuaire::{ … }` gagne
`bloquer_ami, nom_appareil_valide, regenerer_code, retirer_ami, revoquer_appareil, Blocage, Retrait`
(ordre alphabétique de `rustfmt` : fonctions en minuscules d'abord, puis types).

- [ ] **Étape 7 : lancer les tests**

```bash
cd spike && cargo test -p sky-compte && cargo test -p sky-probe && cargo clippy --all-targets -- -D warnings
```
Attendu : PASS ; les six tests de `amis_test.rs` verts ; `sky-probe` : même nombre de tests qu'avant.

- [ ] **Étape 8 : prouver par neutralisation (une à la fois)**

Appliquer les neutralisations écrites en tête de `la_synchronisation_lit_friendship_id…`,
`retirer_ami_vise…`, `un_identifiant_d_amitie_nul…`, `bloquer_ami_poste…`,
`regenerer_code_rend…`. Pour `revoquer_appareil_revoque…` : faire rendre `Ok(())` à
`revoquer_appareil` sans appel réseau — le test rougit sur `revoked_at`. Noter pour chacune ce qui
a rougi et vérifier que rien d'autre n'a rougi.

- [ ] **Étape 9 : commiter et pousser**

```bash
git add spike/crates/sky-compte/src/annuaire.rs
git add spike/crates/sky-compte/src/lib.rs
git add spike/crates/sky-compte/tests/annuaire_test.rs
git add spike/crates/sky-compte/tests/amis_test.rs
git add spike/crates/sky-compte/tests/faux_serveur/mod.rs
git add spike/crates/sky-probe/src/rendez_vous.rs
git diff --cached --stat
git commit -m "feat: sky-compte retire, bloque, regenere le code et revoque un appareil"
git push
```

---

## Task 4 : `sky-partage` — la crate, le rendez-vous déplacé, l'arrêt

**Ce que la tâche fait.** Elle crée la crate, y **déplace** `rendez_vous.rs` avec ses tests (`git
mv`, contenu inchangé à l'octet près), et y ajoute ce dont l'application aura besoin pour
interrompre une attente : un signal d'arrêt et une horloge qui rend la main dès qu'il est levé. Aucune
modification de `interroger` ni de ses tests : l'arrêt passe par la fermeture `synchroniser` (une
erreur **fatale** au sens d'`ErreurDeSynchronisation`) et par l'horloge.

**Compte des tests — avant/après.** Avant : `sky-probe` porte 37 tests (19 dans `cmd_compte.rs`, 1
dans `cmd_host.rs`, 17 dans `rendez_vous.rs` — relevé par `grep -c "#\[test\]"`). Après : `sky-probe`
20, `sky-partage` 17 déplacés + 3 nouveaux. Les **noms** des 17 ne changent pas (même chemin de
module `rendez_vous::tests::…`) : la preuve est un `diff` des listes, étape 7.

**Files:**
- Create: `spike/crates/sky-partage/Cargo.toml`, `spike/crates/sky-partage/src/lib.rs`, `spike/crates/sky-partage/src/arret.rs`
- Move: `spike/crates/sky-probe/src/rendez_vous.rs` → `spike/crates/sky-partage/src/rendez_vous.rs`
- Modify: `spike/crates/sky-probe/Cargo.toml`, `spike/crates/sky-probe/src/main.rs:10` (retrait de `mod rendez_vous;`)
- Modify: `spike/crates/sky-probe/src/cmd_host.rs:28-30`, `spike/crates/sky-probe/src/cmd_view.rs:24-26` (lignes `use`)
- Modify: `spike/Cargo.lock`

**Interfaces:**
- Consumes : `sky_compte::{ErreurCompte, Etat}` ; `rendez_vous::{Horloge, ErreurDeSynchronisation, interroger}` (déplacés tels quels).
- Produces (`sky_partage::arret`, réexporté à la racine) :
  - `#[derive(Clone, Default)] pub struct Arret` ; `Arret::nouveau() -> Arret`, `Arret::demander(&self)`, `Arret::est_demande(&self) -> bool`
  - `pub const PAS_D_ATTENTE: Duration` (50 ms)
  - `pub struct HorlogeArretable<'a>` ; `HorlogeArretable::demarrer(arret: &'a Arret) -> HorlogeArretable<'a>` ; `impl Horloge`
  - `pub enum ErreurAttente { Compte(ErreurCompte), Arrete }` ; `impl ErreurDeSynchronisation` (les deux fatales si `Compte(Refuse)` ou `Arrete`)
  - `pub fn synchroniser_sauf_arret<'a, S>(arret: &'a Arret, synchroniser: S) -> impl FnMut(Option<&Etat>) -> Result<Etat, ErreurAttente> + 'a where S: FnMut(Option<&Etat>) -> Result<Etat, ErreurCompte> + 'a`
  - `sky_partage::rendez_vous::*` : exactement l'API actuelle de `sky-probe/src/rendez_vous.rs`.

- [ ] **Étape 1 : relever la liste des tests avant le déplacement**

```bash
cd spike && cargo test -p sky-probe -- --list 2>/dev/null | grep ": test$" | sort > target/tests-avant.txt; wc -l target/tests-avant.txt
```
(`spike/target/` est ignoré par git : le relevé n'entre jamais dans un commit.) Attendu : **37**.

- [ ] **Étape 2 : créer la crate et déplacer le module**

`spike/crates/sky-partage/Cargo.toml` :

```toml
[package]
name = "sky-partage"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
publish.workspace = true

[dependencies]
anyhow.workspace = true
sky-compte = { version = "0.0.0", path = "../sky-compte" }
sky-net = { version = "0.0.0", path = "../sky-net" }
sky-crypto = { version = "0.0.0", path = "../sky-crypto" }

# Les tests du rendez-vous fabriquent des `AppareilDAmi`, dont la clé
# publique voyage en base64 standard — la forme que rend le site.
[dev-dependencies]
base64.workspace = true
```

```bash
git mv spike/crates/sky-probe/src/rendez_vous.rs spike/crates/sky-partage/src/rendez_vous.rs
```

`spike/crates/sky-partage/src/lib.rs` :

```rust
//! `sky-partage` : la négociation d'une connexion par la boîte aux lettres,
//! puis la diffusion et la réception (jalon 1, décision D5 de la spec).
//! Utilisée par `sky-probe` (affichage terminal) et par l'application
//! (`sky-app`). Elle ne fait AUCUNE sortie terminal : elle rend des
//! événements typés et accepte un signal d'arrêt.

pub mod arret;
pub mod rendez_vous;

pub use arret::{synchroniser_sauf_arret, Arret, ErreurAttente, HorlogeArretable, PAS_D_ATTENTE};
```

Dans `spike/crates/sky-probe/Cargo.toml`, `[dependencies]` gagne :

```toml
sky-partage = { version = "0.0.0", path = "../sky-partage" }
```

Dans `spike/crates/sky-probe/src/main.rs`, supprimer la ligne `mod rendez_vous;`.

Dans `cmd_host.rs`, remplacer `use crate::rendez_vous::{` par `use sky_partage::rendez_vous::{` ;
dans `cmd_view.rs`, de même. Rien d'autre.

- [ ] **Étape 3 : vérifier que le déplacement seul compile et que les tests ont suivi**

```bash
cd spike && cargo test -p sky-partage && cargo test -p sky-probe
```
Attendu : `sky-partage` : **17** tests verts ; `sky-probe` : **20**. Si `sky-partage` refuse de
compiler, lire l'erreur : les seuls changements permis sont ceux de l'étape 2.

- [ ] **Étape 4 : écrire les tests de l'arrêt qui échouent**

Créer `spike/crates/sky-partage/src/arret.rs` avec seulement le module de tests :

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::rendez_vous::{interroger, Horloge};
    use std::time::{Duration, Instant};

    #[derive(Default)]
    struct HorlogeFictive {
        maintenant: Duration,
        attentes: Vec<Duration>,
    }

    impl Horloge for HorlogeFictive {
        fn ecoule(&self) -> Duration {
            self.maintenant
        }
        fn attendre(&mut self, duree: Duration) {
            self.maintenant += duree;
            self.attentes.push(duree);
        }
    }

    fn etat_vide() -> Etat {
        Etat {
            version: 1,
            code: "ABCDEFGH".to_string(),
            amis: Vec::new(),
            demandes: Vec::new(),
            listes: Vec::new(),
            appareils: Vec::new(),
            enveloppes: Vec::new(),
        }
    }

    #[test]
    fn l_attente_rend_la_main_des_que_l_arret_est_demande() {
        // Neutralisation : un `sleep(duree)` d'un seul tenant dans
        // `attendre` — l'attente dure 5 s, le test rougit.
        let arret = Arret::nouveau();
        let depuis_un_fil = arret.clone();
        let fil = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            depuis_un_fil.demander();
        });
        let debut = Instant::now();
        HorlogeArretable::demarrer(&arret).attendre(Duration::from_secs(5));
        fil.join().unwrap();
        assert!(debut.elapsed() < Duration::from_secs(1), "attente de {:?}", debut.elapsed());
    }

    #[test]
    fn sans_arret_l_attente_dure_ce_qu_on_lui_demande() {
        // Neutralisation : sortir de la boucle au premier tour — l'attente
        // rend la main aussitôt, le test rougit.
        let arret = Arret::nouveau();
        let debut = Instant::now();
        HorlogeArretable::demarrer(&arret).attendre(Duration::from_millis(200));
        assert!(debut.elapsed() >= Duration::from_millis(200));
    }

    #[test]
    fn un_arret_interrompt_interroger_au_tour_suivant_sans_retenter() {
        // L'arrêt est une erreur FATALE : `interroger` ne la retente pas.
        // Neutralisation : `ErreurAttente::Arrete => false` dans
        // `est_fatale` — trois tentatives au lieu d'une, trois attentes.
        let arret = Arret::nouveau();
        let mut appels = 0u32;
        let mut horloge = HorlogeFictive::default();
        let issue = interroger(
            None,
            synchroniser_sauf_arret(&arret, |_| {
                appels += 1;
                arret.demander();
                Ok(etat_vide())
            }),
            |_| None::<()>,
            &mut horloge,
            Duration::from_secs(2),
            Duration::from_secs(60),
        );
        assert!(matches!(issue, Err(ErreurAttente::Arrete)));
        assert_eq!(appels, 1, "la fermeture réelle n'est plus appelée après l'arrêt");
        assert_eq!(horloge.attentes.len(), 1, "une seule attente, entre le succès et l'arrêt");
    }
}
```

Lancer :
```bash
cd spike && cargo test -p sky-partage arret
```
Attendu : ÉCHEC à la compilation — `Arret`, `HorlogeArretable`, `synchroniser_sauf_arret`
introuvables.

- [ ] **Étape 5 : implémenter**

En tête de `spike/crates/sky-partage/src/arret.rs`, au-dessus du module de tests :

```rust
//! L'arrêt : ce qui permet à l'application d'interrompre une attente de
//! 30 minutes (hôte) ou de 60 secondes (spectateur), une négociation ou un
//! flux, sans toucher à `interroger` ni à ses tests (déplacés tels quels).
//!
//! Deux pièces. L'HORLOGE rend la main dès que l'arrêt est demandé, au lieu
//! de dormir ses 2 s. La fermeture `synchroniser_sauf_arret` rend alors une
//! erreur FATALE au tour suivant, qu'`interroger` ne retente pas.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use sky_compte::{ErreurCompte, Etat};

use crate::rendez_vous::{ErreurDeSynchronisation, Horloge};

/// Signal d'arrêt partagé entre le fil qui partage et celui qui le demande
/// (le bouton Arrêter). Cloner rend un signal LIÉ, pas une copie.
#[derive(Clone, Default)]
pub struct Arret(Arc<AtomicBool>);

impl Arret {
    pub fn nouveau() -> Arret {
        Arret::default()
    }

    pub fn demander(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn est_demande(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Granularité de l'attente : l'arrêt est vu au plus 50 ms après la
/// demande. Point de départ argumenté, pas une mesure : sous le seuil où un
/// clic « Arrêter » paraît ignoré, et sans réveil inutile toutes les ms.
pub const PAS_D_ATTENTE: Duration = Duration::from_millis(50);

/// L'horloge réelle, interruptible. Remplace `HorlogeReelle` partout où un
/// arrêt doit être possible ; sans arrêt demandé, elle attend exactement
/// autant qu'elle.
pub struct HorlogeArretable<'a> {
    debut: Instant,
    arret: &'a Arret,
}

impl<'a> HorlogeArretable<'a> {
    pub fn demarrer(arret: &'a Arret) -> HorlogeArretable<'a> {
        HorlogeArretable { debut: Instant::now(), arret }
    }
}

impl Horloge for HorlogeArretable<'_> {
    fn ecoule(&self) -> Duration {
        self.debut.elapsed()
    }

    fn attendre(&mut self, duree: Duration) {
        let fin = Instant::now() + duree;
        while !self.arret.est_demande() {
            let reste = fin.saturating_duration_since(Instant::now());
            if reste.is_zero() {
                break;
            }
            std::thread::sleep(reste.min(PAS_D_ATTENTE));
        }
    }
}

/// Erreur d'une attente arrêtable : celle du compte, ou l'arrêt lui-même.
#[derive(Debug)]
pub enum ErreurAttente {
    Compte(ErreurCompte),
    Arrete,
}

impl ErreurDeSynchronisation for ErreurAttente {
    fn est_fatale(&self) -> bool {
        match self {
            ErreurAttente::Compte(e) => e.est_fatale(),
            // Un arrêt ne se « résout » pas en réessayant : le retenter
            // ferait attendre l'utilisateur trois tours de plus.
            ErreurAttente::Arrete => true,
        }
    }
}

/// Enveloppe `synchroniser` : si l'arrêt est demandé, rend `Arrete` SANS
/// appeler le site — une synchronisation de plus consommerait peut-être une
/// enveloppe que plus personne n'attend.
pub fn synchroniser_sauf_arret<'a, S>(
    arret: &'a Arret,
    mut synchroniser: S,
) -> impl FnMut(Option<&Etat>) -> Result<Etat, ErreurAttente> + 'a
where
    S: FnMut(Option<&Etat>) -> Result<Etat, ErreurCompte> + 'a,
{
    move |precedent: Option<&Etat>| {
        if arret.est_demande() {
            return Err(ErreurAttente::Arrete);
        }
        synchroniser(precedent).map_err(ErreurAttente::Compte)
    }
}
```

- [ ] **Étape 6 : lancer**

```bash
cd spike && cargo test -p sky-partage && cargo clippy --all-targets -- -D warnings
```
Attendu : **20** tests verts (17 déplacés + 3).

- [ ] **Étape 7 : prouver que les tests ont suivi, et qu'aucun ne s'est perdu**

```bash
cd spike && cargo test -p sky-probe -- --list 2>/dev/null | grep ": test$" | sort > target/probe-apres.txt
cargo test -p sky-partage -- --list 2>/dev/null | grep ": test$" | grep "^rendez_vous::" | sort > target/partage-apres.txt
cat target/probe-apres.txt target/partage-apres.txt | sort | diff target/tests-avant.txt -
```
Attendu : **aucune ligne** en sortie de `diff` (les 37 noms d'avant = les 20 restés + les 17
déplacés). Puis :
```bash
git diff --cached -M --stat
```
Attendu : `rendez_vous.rs` apparaît comme **renommé à 100 %** (`{sky-probe => sky-partage}/src/rendez_vous.rs`).
Un pourcentage inférieur signifie que le fichier a été retouché : le rétablir.

- [ ] **Étape 8 : prouver par neutralisation (une à la fois)** les trois tests d'`arret.rs`, selon
leurs commentaires. Noter ce qui rougit.

- [ ] **Étape 9 : commiter et pousser**

```bash
git add spike/crates/sky-partage/Cargo.toml
git add spike/crates/sky-partage/src/lib.rs
git add spike/crates/sky-partage/src/arret.rs
git add spike/crates/sky-partage/src/rendez_vous.rs
git add spike/crates/sky-probe/Cargo.toml
git add spike/crates/sky-probe/src/main.rs
git add spike/crates/sky-probe/src/cmd_host.rs
git add spike/crates/sky-probe/src/cmd_view.rs
git add spike/Cargo.lock
git diff --cached -M --stat
git commit -m "refactor: le rendez-vous quitte sky-probe pour sky-partage, avec un signal d arret"
git push
```
(Le `git mv` a déjà indexé la suppression côté `sky-probe` ; le `git add` du nouveau chemin la
complète.)

---

## Task 5 : `sky-partage` — hôte et spectateur ; `sky-probe` en affichage

**Ce que la tâche fait.** Le reste de la colle de `cmd_host.rs` et `cmd_view.rs` passe dans
`sky-partage` : l'établissement, la diffusion (capture, encodage, régulation, envoi), la réception
et ses mesures. **La règle de transformation est mécanique et vérifiable :** chaque `println!` du
code déplacé devient un `Evenement` ; chaque `println!(…) ; return Ok(())` devient `return
Ok(Fin::…)` ; chaque `?` reste un `?` (vers `ErreurPartage::Autre`) ; chaque `.map_err(erreur_compte)?`
devient `.map_err(ErreurPartage::Compte)?`. `sky-probe host`/`view` ne gardent que les textes, **les
mêmes caractère pour caractère**, affichés à la réception de chaque événement. Le spectateur de
`sky-partage` **jette** le flux ; c'est `sky-probe view` qui lui fournit un fichier.

**Arbitrages de cette tâche (tranchés ici, à relire) :**
- `sky-probe view --out` garde sa valeur par défaut `recu.h265` : le comportement ne change pas
  (spec §8, « exactement comme à l'essai réel »). « L'écriture dans un fichier devient une option »
  (spec §3) se lit au niveau de `sky-partage` : `regarder` reçoit une fabrique de puits, qui rend
  `None` dans l'application.
- La fabrique de puits est appelée **après** l'événement `Connecte` : `sky-probe view` crée donc le
  fichier au même moment qu'avant (jamais sur un échec de négociation).
- La source synthétique (`TextureSynthetique`) reste dans `sky-probe` : `sky-partage` reçoit une
  fabrique de type `fn(&ID3D11Device, u32, u32) -> anyhow::Result<Box<dyn Images>>`.
- `FPS` (60) vit désormais dans `sky-partage` ; `cmd_encode::FPS` le reprend (une seule source).

**Tests déplacés :** aucun ici (les 17 du rendez-vous ont bougé en T4 ; `cmd_host.rs` garde son test
`l_echec_local_ne_promet_aucune_reprise_et_renvoie_a_view`, avec `message_echec_local`, qui est un
texte de terminal). Tests nouveaux : 4 (`reception`), 1 (`spectateur`), 1 (`hote`) dans
`sky-partage` ; 3 dans `sky-probe` (textes).

**Files:**
- Modify: `spike/crates/sky-partage/Cargo.toml`, `spike/crates/sky-partage/src/lib.rs`
- Create: `spike/crates/sky-partage/src/evenement.rs`, `etablissement.rs`, `reception.rs`, `hote.rs`, `spectateur.rs`
- Rewrite: `spike/crates/sky-probe/src/cmd_host.rs`, `spike/crates/sky-probe/src/cmd_view.rs`
- Modify: `spike/crates/sky-probe/src/cmd_encode.rs:33` (`FPS`)
- Modify: `spike/Cargo.lock`

**Interfaces:**
- Consumes : `rendez_vous::{interroger, offres_recevables, reponse_a_l_offre, session_de, echec_local, CADENCE, FENETRE_HOTE, ATTENTE_SPECTATEUR}`, `arret::*` (T4) ; `sky_compte::{deposer, relever, resoudre_ami, Ami, Coffre, Config, ErreurCompte, Etat}` ; `sky_net::{LinkEvent, Pacer, PeerLink}` ; `sky_capture::{wgc::WgcCapture, CapturedFrame}` ; `sky_encode::{nvenc::NvencEncoder, Codec}`.
- Produces (tout réexporté à la racine de `sky_partage`) :
  - `evenement.rs` : `pub enum Evenement { Pret, Disponible { fenetre: Duration }, DemandeEcartee { raison: String }, EchecLocal { raison: String }, DemandeRecue { expediteur_device_id: i64, apres: Duration, synchronisations: u32 }, DemandeEnvoyee { nom: String, deposes: usize, appareils: usize, attente: Duration }, ReponseRecue { apres: Duration, synchronisations: u32 }, Negociation, Connecte { en: Duration, depuis_le_lancement: Option<Duration> }, Diffusion { largeur: u32, hauteur: u32, codec: Codec, plancher_mbps: u32, plafond_mbps: u32 }, Mesures(Mesures) }` ; `pub enum Mesures { Envoi { debit_mbps: f64, cible_mbps: f64, images_sautees: u64, rtt_ms: f64 }, Reception { debit_mbps: f64, images_par_s: u64, gigue_ms: f64 } }` ; `pub struct Quantiles { pub p50: f64, pub p99: f64, pub echantillons: usize }` ; `pub struct BilanEnvoi { … }`, `pub struct BilanReception { … }`, `pub enum Bilan { Envoi(BilanEnvoi), Reception(BilanReception) }` ; `pub struct Diagnostic { pub ice_connecte: bool, pub emis: u64, pub recus: u64, pub erreurs: u64, pub vers_local: u64, pub vers_internet: u64, pub erreurs_socket: u64, pub delai: Duration }` ; `pub enum Fin { Arrete, DureeEcoulee(Box<Bilan>), AucunAppareilLocal, AucuneDemande, ReponseRefusee, AucunAppareilChezLAmi { nom: String }, DemandeRefusee { nom: String, appareils: usize }, PasDeReponse { nom: String }, NegociationRompue(String), EtablissementEchoue(Diagnostic), TamponSature { morceau: usize, morceaux: usize }, LienTombe(String) }` ; `pub enum ErreurPartage { Compte(ErreurCompte), Autre(anyhow::Error) }` avec `impl From<anyhow::Error>`.
  - `etablissement.rs` : `pub const DELAI_ETABLISSEMENT: Duration` (25 s) ; `pub enum Etablissement { Ouvert(Duration), Rompu(String), Delai(Diagnostic), Arrete }` ; `pub fn etablir(link: &mut PeerLink, arret: &Arret) -> anyhow::Result<Etablissement>` ; `pub fn diagnostic(link: &PeerLink) -> Diagnostic`.
  - `reception.rs` : `#[derive(Default)] pub struct Reception` ; `absorber<'d>(&mut self, d: &'d [u8], arrivee_us: u64) -> Option<&'d [u8]>`, `octets(&self) -> u64`, `images(&self) -> u64`, `gigue_ms(&self) -> f64`, `dernier_horodatage_emission(&self) -> Option<u64>`, `transit_ms(&mut self) -> Option<Quantiles>`.
  - `hote.rs` : `pub const FPS: u32 = 60`, `pub const EN_TETE_MORCEAU: usize = 9`, `pub const TAILLE_MORCEAU_PAYLOAD: usize`, `pub const BUDGET_RETRY_ENVOI: Duration`, `pub fn epoch_us() -> u64`, `pub trait Images { fn prochaine_image(&mut self) -> anyhow::Result<CapturedFrame>; }`, `pub type FabriqueSynthetique = fn(&ID3D11Device, u32, u32) -> anyhow::Result<Box<dyn Images>>`, `pub enum SourceImages { Ecran, Synthetique { largeur: u32, hauteur: u32, fabrique: FabriqueSynthetique } }`, `pub struct ParametresHote { pub codec: Codec, pub plafond_mbps: u32, pub plancher_mbps: u32, pub moniteur: usize, pub source: SourceImages, pub duree_max: Option<Duration> }`, `pub fn heberger(config: &Config, coffre: &Coffre, synchroniser: impl FnMut(Option<&Etat>) -> Result<Etat, ErreurCompte>, p: ParametresHote, arret: &Arret, evenements: &mut dyn FnMut(Evenement)) -> Result<Fin, ErreurPartage>`.
  - `spectateur.rs` : `pub enum Designation<'a> { Texte(&'a str), Identifiant(i64) }`, `pub fn trouver_ami<'e>(etat: &'e Etat, designation: &Designation<'_>) -> Result<&'e Ami, ErreurCompte>`, `pub struct ParametresSpectateur<'a> { pub ami: Designation<'a>, pub duree_max: Option<Duration> }`, `pub type Puits = Box<dyn std::io::Write>`, `pub fn regarder(config: &Config, coffre: &Coffre, synchroniser: impl FnMut(Option<&Etat>) -> Result<Etat, ErreurCompte>, p: ParametresSpectateur<'_>, ouvrir_puits: impl FnOnce() -> anyhow::Result<Option<Puits>>, arret: &Arret, evenements: &mut dyn FnMut(Evenement)) -> Result<Fin, ErreurPartage>`.

- [ ] **Étape 1 : dépendances**

`spike/crates/sky-partage/Cargo.toml`, `[dependencies]` gagne :

```toml
sky-capture = { version = "0.0.0", path = "../sky-capture" }
sky-encode = { version = "0.0.0", path = "../sky-encode" }
# Seulement pour nommer `ID3D11Device` dans la fabrique de la source
# synthétique : c'est le device de la capture qui la crée.
windows = { version = "0.62", features = ["Win32_Graphics_Direct3D11"] }
```

- [ ] **Étape 2 : écrire les tests qui échouent — réception et désignation**

Créer `spike/crates/sky-partage/src/reception.rs` avec seulement :

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Un morceau tel que l'hôte l'envoie : 8 octets d'horodatage
    /// d'émission, 1 octet drapeau, la charge.
    fn morceau(emission_us: u64, premier: bool, charge: &[u8]) -> Vec<u8> {
        let mut m = emission_us.to_le_bytes().to_vec();
        m.push(u8::from(premier));
        m.extend_from_slice(charge);
        m
    }

    #[test]
    fn seul_le_premier_morceau_compte_une_image() {
        // Neutralisation : compter une image par morceau — `images` vaut 2.
        let mut r = Reception::default();
        r.absorber(&morceau(0, true, b"abc"), 1_000);
        r.absorber(&morceau(0, false, b"de"), 1_100);
        assert_eq!(r.images(), 1);
        assert_eq!(r.octets(), 5);
    }

    #[test]
    fn la_charge_rendue_est_sans_en_tete_et_un_message_trop_court_est_ignore() {
        // Neutralisation : `>=` au lieu de `>` sur la longueur — le message
        // de 9 octets rend `Some(&[])` au lieu de `None`.
        let mut r = Reception::default();
        let m = morceau(0, true, b"xyz");
        assert_eq!(r.absorber(&m, 5), Some(&b"xyz"[..]));
        assert_eq!(r.absorber(&[0u8; EN_TETE_MORCEAU], 6), None);
        assert_eq!(r.octets(), 3);
    }

    #[test]
    fn la_gigue_suit_la_rfc_3550() {
        // Émissions à 0 et 10 000 µs, arrivées à 1 000 et 13 000 µs : écart
        // 2 000 µs, lissé au 1/16 → 125 µs. Neutralisation : diviser par 8.
        let mut r = Reception::default();
        r.absorber(&morceau(0, true, b"a"), 1_000);
        r.absorber(&morceau(10_000, true, b"b"), 13_000);
        assert!((r.gigue_ms() - 0.125).abs() < 1e-9, "gigue {}", r.gigue_ms());
    }

    #[test]
    fn le_transit_est_mesure_image_par_image() {
        let mut r = Reception::default();
        assert_eq!(r.transit_ms(), None);
        r.absorber(&morceau(0, true, b"a"), 2_000);
        r.absorber(&morceau(0, false, b"b"), 9_000);
        let q = r.transit_ms().unwrap();
        assert_eq!(q.echantillons, 1, "un seul échantillon : le second morceau n'ouvre pas d'image");
        assert!((q.p50 - 2.0).abs() < 1e-9);
    }
}
```

Créer `spike/crates/sky-partage/src/spectateur.rs` avec seulement :

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use sky_compte::Ami;

    fn ami(id: i64, nom: &str) -> Ami {
        Ami { id, friendship_id: id, discord_name: nom.to_string(), appareils: Vec::new() }
    }

    #[test]
    fn un_identifiant_ne_se_confond_jamais_avec_un_nom() {
        // L'application désigne l'ami par son identifiant. `resoudre_ami`
        // refuserait ici l'ambiguïté (un ami NOMMÉ « 7 », un autre
        // D'IDENTIFIANT 7). Neutralisation : passer `Identifiant` par
        // `resoudre_ami(etat, &id.to_string())` — erreur, le test rougit.
        let etat = Etat {
            version: 1,
            code: "ABCDEFGH".to_string(),
            amis: vec![ami(3, "7"), ami(7, "bob")],
            demandes: Vec::new(),
            listes: Vec::new(),
            appareils: Vec::new(),
            enveloppes: Vec::new(),
        };
        assert_eq!(trouver_ami(&etat, &Designation::Identifiant(7)).unwrap().discord_name, "bob");
        assert!(trouver_ami(&etat, &Designation::Texte("7")).is_err());
        assert!(trouver_ami(&etat, &Designation::Identifiant(99)).is_err());
    }
}
```

- [ ] **Étape 3 : lancer, vérifier l'échec**

```bash
cd spike && cargo test -p sky-partage
```
Attendu : ÉCHEC à la compilation (`Reception`, `trouver_ami` introuvables) — après avoir déclaré
`pub mod reception; pub mod spectateur;` dans `lib.rs`.

- [ ] **Étape 4 : implémenter `evenement.rs`**

```rust
//! Ce que `heberger` et `regarder` font savoir à qui les appelle. Aucune
//! phrase ici : les textes vivent chez l'appelant (`sky-probe` : terminal ;
//! `sky-app` : interface). Aucun champ ne porte d'adresse.

use std::time::Duration;

use sky_compte::ErreurCompte;
use sky_encode::Codec;

#[derive(Debug, Clone, PartialEq)]
pub enum Evenement {
    /// Hôte : paramètres validés, avant tout accès au coffre ou au réseau.
    Pret,
    /// Hôte : disponible, une demande sera honorée pendant `fenetre`.
    Disponible { fenetre: Duration },
    /// Hôte : une offre recevable refusée par `PeerLink::repondant` (contenu du bloc).
    DemandeEcartee { raison: String },
    /// Hôte : `repondant` a échoué pour une cause LOCALE ; la demande est perdue.
    EchecLocal { raison: String },
    /// Hôte : une demande d'ami retenue.
    DemandeRecue { expediteur_device_id: i64, apres: Duration, synchronisations: u32 },
    /// Spectateur : offre déposée pour `deposes` des `appareils` de l'ami.
    DemandeEnvoyee { nom: String, deposes: usize, appareils: usize, attente: Duration },
    /// Spectateur : réponse de l'ami relevée.
    ReponseRecue { apres: Duration, synchronisations: u32 },
    /// Négociation en cours (hôte : réponse déposée ; spectateur : réponse acceptée).
    Negociation,
    /// Canal de données ouvert.
    Connecte { en: Duration, depuis_le_lancement: Option<Duration> },
    /// Hôte : la diffusion commence.
    Diffusion { largeur: u32, hauteur: u32, codec: Codec, plancher_mbps: u32, plafond_mbps: u32 },
    /// Une fois par seconde pendant le flux.
    Mesures(Mesures),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Mesures {
    Envoi { debit_mbps: f64, cible_mbps: f64, images_sautees: u64, rtt_ms: f64 },
    /// `images_par_s` : images arrivées depuis la mesure précédente (~1 s).
    Reception { debit_mbps: f64, images_par_s: u64, gigue_ms: f64 },
}

/// Médiane et 99e centile, en millisecondes.
#[derive(Debug, Clone, PartialEq)]
pub struct Quantiles {
    pub p50: f64,
    pub p99: f64,
    pub echantillons: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BilanEnvoi {
    pub duree_s: f64,
    pub images_encodees: u64,
    pub images_sautees: u64,
    pub encodage_ms: Option<Quantiles>,
    pub envoyes_octets: u64,
    pub cible_finale_bps: u32,
    pub retours: u64,
    pub echecs_envoi: u64,
    pub tentatives_envoi: u64,
    pub rtt_ms: Option<Quantiles>,
    pub vers_internet: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BilanReception {
    pub duree_s: f64,
    pub images: u64,
    pub octets: u64,
    pub transit_ms: Option<Quantiles>,
    pub gigue_ms: f64,
    pub vers_internet: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Bilan {
    Envoi(BilanEnvoi),
    Reception(BilanReception),
}

/// Ce que `etablir` a OBSERVÉ quand le canal ne s'est pas ouvert à temps —
/// jamais une cause déduite (leçon du 23/08 : un diagnostic n'énonce que ce
/// qu'il a mesuré).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// ICE a trouvé un chemin : c'est la poignée de main chiffrée qui a échoué.
    pub ice_connecte: bool,
    pub emis: u64,
    pub recus: u64,
    pub erreurs: u64,
    pub vers_local: u64,
    pub vers_internet: u64,
    pub erreurs_socket: u64,
    pub delai: Duration,
}

/// Les fins NORMALES d'un partage — tout ce qui, dans `sky-probe`, se
/// terminait par un message puis `return Ok(())`.
#[derive(Debug, Clone, PartialEq)]
pub enum Fin {
    Arrete,
    /// `duree_max` atteinte (`sky-probe --seconds`). Jamais dans l'application.
    DureeEcoulee(Box<Bilan>),
    AucunAppareilLocal,
    /// Hôte : `FENETRE_HOTE` écoulée sans demande.
    AucuneDemande,
    /// Hôte : le site a refusé la réponse.
    ReponseRefusee,
    AucunAppareilChezLAmi { nom: String },
    /// Spectateur : le site a refusé l'offre pour tous les appareils de l'ami.
    DemandeRefusee { nom: String, appareils: usize },
    /// Spectateur : `ATTENTE_SPECTATEUR` écoulée sans réponse.
    PasDeReponse { nom: String },
    /// `etablir` : le lien a signalé un échec.
    NegociationRompue(String),
    /// `etablir` : canal non ouvert en `DELAI_ETABLISSEMENT`.
    EtablissementEchoue(Diagnostic),
    /// Hôte : tampon d'émission plein plus de `BUDGET_RETRY_ENVOI` —
    /// « la connexion était trop lente pour la vidéo » (écart 7).
    TamponSature { morceau: usize, morceaux: usize },
    /// Le lien est tombé pendant le flux.
    LienTombe(String),
}

/// Les fins ANORMALES : tout ce qui, dans `sky-probe`, remontait par `?`.
#[derive(Debug)]
pub enum ErreurPartage {
    Compte(ErreurCompte),
    Autre(anyhow::Error),
}

impl From<anyhow::Error> for ErreurPartage {
    fn from(e: anyhow::Error) -> ErreurPartage {
        ErreurPartage::Autre(e)
    }
}
```

- [ ] **Étape 5 : implémenter `reception.rs`** (au-dessus du module de tests)

```rust
//! La comptabilité du spectateur, extraite de la boucle de `cmd_view` (jalon
//! C2) pour être testable sans réseau : ce qui compte une image, le débit,
//! la gigue RFC 3550 et le transit. Rend la charge utile ; c'est l'appelant
//! qui décide de l'écrire ou de la jeter.

use crate::evenement::Quantiles;
use crate::hote::EN_TETE_MORCEAU;

#[derive(Default)]
pub struct Reception {
    octets: u64,
    images: u64,
    dernier_emission: Option<u64>,
    derniere_arrivee: Option<u64>,
    gigue_us: f64,
    transits_us: Vec<i64>,
}

impl Reception {
    /// Absorbe un message du canal. `None` pour un message trop court pour
    /// porter un en-tête complet (ignoré, comme au C2) ; sinon la charge
    /// utile, sans son en-tête.
    ///
    /// Seul le PREMIER morceau d'une image (drapeau non nul) porte les
    /// statistiques par image : compteur, transit, gigue — calculés image à
    /// image, pas morceau à morceau (ce qui mesurerait notre découpage).
    pub fn absorber<'d>(&mut self, d: &'d [u8], arrivee_us: u64) -> Option<&'d [u8]> {
        if d.len() <= EN_TETE_MORCEAU {
            return None;
        }
        let emission = u64::from_le_bytes(d[..8].try_into().expect("8 octets d'horodatage"));
        let premier_morceau = d[8] != 0;
        let charge = &d[EN_TETE_MORCEAU..];
        if premier_morceau {
            self.transits_us.push(arrivee_us as i64 - emission as i64);
            if let (Some(prec_emis), Some(prec_arr)) = (self.dernier_emission, self.derniere_arrivee) {
                let delta_emission = emission as i64 - prec_emis as i64;
                let delta_arrivee = arrivee_us as i64 - prec_arr as i64;
                let ecart = (delta_arrivee - delta_emission).unsigned_abs() as f64;
                self.gigue_us += (ecart - self.gigue_us) / 16.0;
            }
            self.dernier_emission = Some(emission);
            self.derniere_arrivee = Some(arrivee_us);
            self.images += 1;
        }
        self.octets += charge.len() as u64;
        Some(charge)
    }

    pub fn octets(&self) -> u64 {
        self.octets
    }

    pub fn images(&self) -> u64 {
        self.images
    }

    pub fn gigue_ms(&self) -> f64 {
        self.gigue_us / 1000.0
    }

    /// Horodatage d'émission du dernier paquet vidéo reçu — renvoyé tel quel
    /// à l'hôte, qui en tire un aller-retour réel.
    pub fn dernier_horodatage_emission(&self) -> Option<u64> {
        self.dernier_emission
    }

    /// Médiane et p99 du transit (arrivée − émission), en ms. `None` sans image.
    pub fn transit_ms(&mut self) -> Option<Quantiles> {
        if self.transits_us.is_empty() {
            return None;
        }
        self.transits_us.sort_unstable();
        let centile = |p: usize| {
            let t = &self.transits_us;
            t[(t.len() * p / 100).min(t.len() - 1)] as f64 / 1000.0
        };
        Some(Quantiles { p50: centile(50), p99: centile(99), echantillons: self.transits_us.len() })
    }
}
```

- [ ] **Étape 6 : implémenter `etablissement.rs`**

```rust
//! L'établissement du canal, déplacé de `cmd_host::etablir` (C2). Même délai,
//! même boucle ; le diagnostic rend des NOMBRES observés, et l'arrêt est
//! consulté à chaque tour.

use std::time::{Duration, Instant};

use sky_net::{LinkEvent, PeerLink};

use crate::arret::Arret;
use crate::evenement::Diagnostic;

/// Délai maximal d'établissement, imposé par le document d'architecture
/// (§5.5). Jamais d'attente indéfinie.
pub const DELAI_ETABLISSEMENT: Duration = Duration::from_secs(25);

pub enum Etablissement {
    /// Canal de données ouvert après cette durée.
    Ouvert(Duration),
    /// Le lien a signalé un échec — message déjà sans adresse (`link.rs`).
    Rompu(String),
    /// Délai dépassé : ce qui a été observé.
    Delai(Diagnostic),
    Arrete,
}

/// Boucle jusqu'à ce que le canal de données soit utilisable, ou renonce.
pub fn etablir(link: &mut PeerLink, arret: &Arret) -> anyhow::Result<Etablissement> {
    let debut = Instant::now();
    loop {
        if arret.est_demande() {
            return Ok(Etablissement::Arrete);
        }
        match link.poll()? {
            LinkEvent::Failed(raison) => return Ok(Etablissement::Rompu(raison)),
            LinkEvent::Connected | LinkEvent::Data(_) | LinkEvent::Idle => {}
        }
        // Le canal de données, pas seulement ICE : c'est lui qui transporte.
        if link.canal_ouvert() {
            return Ok(Etablissement::Ouvert(debut.elapsed()));
        }
        if debut.elapsed() > DELAI_ETABLISSEMENT {
            return Ok(Etablissement::Delai(diagnostic(link)));
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Les compteurs que `cmd_host::diagnostiquer` affichait au C2.
pub fn diagnostic(link: &PeerLink) -> Diagnostic {
    let (emis, recus, erreurs) = link.trafic();
    let (vers_local, vers_internet) = link.destinations();
    Diagnostic {
        ice_connecte: link.is_connected(),
        emis,
        recus,
        erreurs,
        vers_local,
        vers_internet,
        erreurs_socket: link.erreurs_socket(),
        delai: DELAI_ETABLISSEMENT,
    }
}
```

- [ ] **Étape 7 : implémenter `hote.rs`**

Le fichier reprend `cmd_host.rs` (C2) : constantes (lignes 32-88), `servir_reseau`,
`ResultatEnvoi`, `envoyer_ou_abandonner`, `percentile_f64`, `percentile_u64` (lignes 517-642)
**recopiées à l'identique** (sauf `pub(crate)` → `pub` pour `EN_TETE_MORCEAU`,
`TAILLE_MORCEAU_PAYLOAD`, `epoch_us`, et `pub` sur `BUDGET_RETRY_ENVOI`). La fonction `run` devient
`heberger` + `diffuser`, écrites ci-dessous en entier :

```rust
//! Côté hôte : attendre la demande d'un ami, y répondre, puis capturer,
//! encoder et envoyer le flux. Déplacé de `sky-probe/src/cmd_host.rs` (C2) :
//! chaque `println!` y est devenu un `Evenement`, chaque `return Ok(())` une
//! `Fin`, chaque `?` est resté un `?`.
//!
//! `WgcCapture::next_frame()` → `NvencEncoder::encode()` → `link.send()`,
//! avec le débit réellement piloté par `Pacer::target_bps()`.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sky_capture::wgc::WgcCapture;
use sky_capture::CapturedFrame;
use sky_compte::{deposer, relever, Coffre, Config, ErreurCompte, Etat};
use sky_crypto::Identity;
use sky_encode::{nvenc::NvencEncoder, Codec};
use sky_net::{LinkEvent, Pacer, PeerLink};
use windows::Win32::Graphics::Direct3D11::ID3D11Device;

use crate::arret::{synchroniser_sauf_arret, Arret, ErreurAttente, HorlogeArretable};
use crate::etablissement::{etablir, Etablissement};
use crate::evenement::{Bilan, BilanEnvoi, ErreurPartage, Evenement, Fin, Mesures, Quantiles};
use crate::rendez_vous::{echec_local, interroger, offres_recevables, CADENCE, FENETRE_HOTE};

/// Cadence visée pour l'encodage. `sky-probe` la reprend (`cmd_encode::FPS`).
pub const FPS: u32 = 60;

// … ici, recopiées de cmd_host.rs (C2) lignes 36-88, à l'identique :
// EN_TETE_MORCEAU (pub), TAILLE_MORCEAU_PAYLOAD (pub),
// GRANULARITE_SERVICE_RESEAU, BUDGET_RETRY_ENVOI (pub), epoch_us (pub).

/// Une source d'images sur le device de la capture — la texture synthétique
/// de `sky-probe` l'implémente.
pub trait Images {
    fn prochaine_image(&mut self) -> anyhow::Result<CapturedFrame>;
}

/// Fabrique la source synthétique sur le device de la capture.
pub type FabriqueSynthetique = fn(&ID3D11Device, u32, u32) -> anyhow::Result<Box<dyn Images>>;

pub enum SourceImages {
    Ecran,
    Synthetique { largeur: u32, hauteur: u32, fabrique: FabriqueSynthetique },
}

pub struct ParametresHote {
    pub codec: Codec,
    /// Débit cible de NVENC — aussi le plafond du Pacer.
    pub plafond_mbps: u32,
    pub plancher_mbps: u32,
    /// Écran capturé (ordre d'`EnumDisplayMonitors`). Le device de cet écran
    /// sert aussi à la source synthétique, comme au C2.
    pub moniteur: usize,
    pub source: SourceImages,
    /// `None` : jusqu'à l'arrêt (application). `Some` : `sky-probe --seconds`.
    pub duree_max: Option<Duration>,
}

pub fn heberger(
    config: &Config,
    coffre: &Coffre,
    mut synchroniser: impl FnMut(Option<&Etat>) -> Result<Etat, ErreurCompte>,
    p: ParametresHote,
    arret: &Arret,
    evenements: &mut dyn FnMut(Evenement),
) -> Result<Fin, ErreurPartage> {
    // Construit AVANT la négociation (C2) : une faute de bornes doit coûter
    // une seconde, pas une attente de FENETRE_HOTE.
    let mut pacer =
        Pacer::new(p.plancher_mbps * 1_000_000, p.plafond_mbps * 1_000_000).map_err(anyhow::Error::from)?;
    evenements(Evenement::Pret);

    // Sans appareil enregistré, personne ne peut nous adresser de demande.
    if coffre.identifiant_appareil().map_err(ErreurPartage::Compte)?.is_none() {
        return Ok(Fin::AucunAppareilLocal);
    }
    let identite = coffre.identite().map_err(ErreurPartage::Compte)?;
    evenements(Evenement::Disponible { fenetre: FENETRE_HOTE });

    let debut_attente = Instant::now();
    let mut synchronisations = 0u32;
    let mut horloge = HorlogeArretable::demarrer(arret);
    let attente = interroger(
        None,
        synchroniser_sauf_arret(arret, |precedent| {
            synchronisations += 1;
            synchroniser(precedent)
        }),
        |etat| {
            // Recevable ne veut pas dire utilisable : si `repondant` refuse le
            // SDP, on essaie l'offre suivante sans interrompre l'attente.
            for offre in offres_recevables(etat, relever(etat, &identite)) {
                match PeerLink::repondant(Identity::generate(), &offre.texte) {
                    Ok((link, reponse)) => return Some((link, reponse, offre.destinataire)),
                    Err(e) => {
                        let raison = e.to_string();
                        // Cause LOCALE : la dire « écartée » ferait porter la
                        // faute à l'ami. Rien n'est retenté (C2, revue I2).
                        if echec_local(&raison) {
                            evenements(Evenement::EchecLocal { raison });
                        } else {
                            evenements(Evenement::DemandeEcartee { raison });
                        }
                    }
                }
            }
            None
        },
        &mut horloge,
        CADENCE,
        FENETRE_HOTE,
    );
    let (mut link, reponse, destinataire) = match attente {
        Ok(Some(retenue)) => retenue,
        Ok(None) => return Ok(Fin::AucuneDemande),
        Err(ErreurAttente::Arrete) => return Ok(Fin::Arrete),
        Err(ErreurAttente::Compte(e)) => return Err(ErreurPartage::Compte(e)),
    };
    evenements(Evenement::DemandeRecue {
        expediteur_device_id: destinataire.id,
        apres: debut_attente.elapsed(),
        synchronisations,
    });

    // L'enveloppe est scellée par `deposer` pour la clé d'ANNUAIRE de
    // l'appareil expéditeur (voir `rendez_vous::destinataire_de_la_reponse`).
    let deposes = deposer(config, coffre, std::slice::from_ref(&destinataire), reponse.as_bytes())
        .map_err(ErreurPartage::Compte)?;
    if deposes == 0 {
        return Ok(Fin::ReponseRefusee);
    }
    evenements(Evenement::Negociation);

    let garde = link.maintenir_mapping()?;
    let duree = match etablir(&mut link, arret)? {
        Etablissement::Ouvert(duree) => duree,
        Etablissement::Rompu(raison) => return Ok(Fin::NegociationRompue(raison)),
        Etablissement::Delai(diagnostic) => return Ok(Fin::EtablissementEchoue(diagnostic)),
        Etablissement::Arrete => return Ok(Fin::Arrete),
    };
    drop(garde);
    evenements(Evenement::Connecte { en: duree, depuis_le_lancement: None });

    diffuser(&mut link, &mut pacer, p, arret, evenements)
}

fn diffuser(
    link: &mut PeerLink,
    pacer: &mut Pacer,
    p: ParametresHote,
    arret: &Arret,
    evenements: &mut dyn FnMut(Evenement),
) -> Result<Fin, ErreurPartage> {
    let mut cap = WgcCapture::new(p.moniteur, None)?;
    let (largeur, hauteur, mut synth): (u32, u32, Option<Box<dyn Images>>) = match p.source {
        SourceImages::Ecran => {
            let attente_max = Instant::now() + Duration::from_secs(5);
            let premiere = loop {
                if let Some(f) = cap.next_frame(Duration::from_millis(200))? {
                    break f;
                }
                if Instant::now() >= attente_max {
                    return Err(anyhow::anyhow!("aucune image capturée en 5 s — l'écran est-il figé ?").into());
                }
            };
            (premiere.width, premiere.height, None)
        }
        SourceImages::Synthetique { largeur, hauteur, fabrique } => {
            let s = fabrique(cap.d3d_device(), largeur, hauteur)?;
            (largeur, hauteur, Some(s))
        }
    };

    let mut enc = NvencEncoder::new(cap.d3d_device(), p.codec, largeur, hauteur, FPS, p.plafond_mbps * 1_000_000)?;
    evenements(Evenement::Diffusion {
        largeur,
        hauteur,
        codec: p.codec,
        plancher_mbps: p.plancher_mbps,
        plafond_mbps: p.plafond_mbps,
    });

    let t0 = Instant::now();
    let fin = p.duree_max.map(|d| t0 + d);
    let periode = Duration::from_micros(1_000_000 / FPS as u64);
    let mut prochaine_image = Instant::now();
    let mut budget_octets = 0.0f64;
    let mut dernier_budget = Instant::now();
    let mut envoyes_octets = 0u64;
    let mut images_encodees = 0u64;
    let mut images_sautees = 0u64;
    let mut retours = 0u64;
    let mut echecs_send = 0u64;
    let mut tentatives_send = 0u64;
    let mut echantillons_encode_us: Vec<u64> = Vec::new();
    let mut dernier_rtt_ms: f64 = 0.0;
    let mut echantillons_rtt: Vec<f64> = Vec::new();
    let mut dernier_feedback = Instant::now();
    let mut fenetre_tentatives = 0u64;
    let mut fenetre_echecs = 0u64;
    let mut dernier_affichage = Instant::now();
    let mut octets_precedent = 0u64;

    loop {
        if arret.est_demande() {
            return Ok(Fin::Arrete);
        }
        if fin.is_some_and(|f| Instant::now() >= f) {
            break;
        }

        // 1. Recharger le budget, plafonné à 250 ms de crédit.
        let maintenant = Instant::now();
        let dt = maintenant.duration_since(dernier_budget).as_secs_f64();
        dernier_budget = maintenant;
        budget_octets += dt * pacer.target_bps() as f64 / 8.0;
        budget_octets = budget_octets.min(pacer.target_bps() as f64 / 8.0 * 0.25);

        // 2. Image suivante, le lien servi PENDANT l'attente.
        let image = match &mut synth {
            Some(s) => {
                loop {
                    let m = Instant::now();
                    if m >= prochaine_image {
                        break;
                    }
                    if let Some(raison) = servir_reseau(link, &mut retours, &mut dernier_rtt_ms, &mut echantillons_rtt)? {
                        return Ok(Fin::LienTombe(raison));
                    }
                    std::thread::sleep(GRANULARITE_SERVICE_RESEAU.min(prochaine_image.saturating_duration_since(m)));
                }
                prochaine_image += periode;
                Some(s.prochaine_image()?)
            }
            None => {
                let mut trouvee = None;
                let echeance = Instant::now() + Duration::from_millis(50);
                while Instant::now() < echeance {
                    if let Some(raison) = servir_reseau(link, &mut retours, &mut dernier_rtt_ms, &mut echantillons_rtt)? {
                        return Ok(Fin::LienTombe(raison));
                    }
                    if let Some(f) = cap.next_frame(GRANULARITE_SERVICE_RESEAU)? {
                        trouvee = Some(f);
                        break;
                    }
                }
                trouvee
            }
        };

        if let Some(image) = image {
            if budget_octets < 0.0 {
                // Budget épuisé : on saute la CAPTURE→ENCODAGE, jamais un
                // paquet déjà produit (GOP infini : le flux resterait cohérent).
                images_sautees += 1;
            } else if let Some(pkt) = enc.encode(&image)? {
                images_encodees += 1;
                budget_octets -= pkt.data.len() as f64;
                echantillons_encode_us.push(pkt.encode_us);

                let horodatage = epoch_us();
                let nb_morceaux = pkt.data.chunks(TAILLE_MORCEAU_PAYLOAD).count().max(1);
                for (i, morceau) in pkt.data.chunks(TAILLE_MORCEAU_PAYLOAD).enumerate() {
                    let mut charge = Vec::with_capacity(EN_TETE_MORCEAU + morceau.len());
                    charge.extend_from_slice(&horodatage.to_le_bytes());
                    charge.push(if i == 0 { 1 } else { 0 });
                    charge.extend_from_slice(morceau);

                    tentatives_send += 1;
                    match envoyer_ou_abandonner(
                        link,
                        &charge,
                        &mut envoyes_octets,
                        &mut echecs_send,
                        &mut fenetre_tentatives,
                        &mut fenetre_echecs,
                        &mut retours,
                        &mut dernier_rtt_ms,
                        &mut echantillons_rtt,
                    )? {
                        ResultatEnvoi::Envoye => {}
                        ResultatEnvoi::Abandonne => {
                            return Ok(Fin::TamponSature { morceau: i + 1, morceaux: nb_morceaux });
                        }
                        ResultatEnvoi::LienTombe(raison) => return Ok(Fin::LienTombe(raison)),
                    }
                }
            }
        }

        // 3. Un dernier service réseau après l'encodage/envoi.
        if let Some(raison) = servir_reseau(link, &mut retours, &mut dernier_rtt_ms, &mut echantillons_rtt)? {
            return Ok(Fin::LienTombe(raison));
        }

        // 4. Nourrir le régulateur de débit à ~10 Hz.
        if dernier_feedback.elapsed() >= Duration::from_millis(100) {
            let perte_pct = if fenetre_tentatives > 0 {
                fenetre_echecs as f32 / fenetre_tentatives as f32 * 100.0
            } else {
                0.0
            };
            pacer.on_feedback(perte_pct, dernier_rtt_ms.round() as u32, dernier_feedback.elapsed());
            fenetre_tentatives = 0;
            fenetre_echecs = 0;
            dernier_feedback = Instant::now();
        }

        // 5. Une mesure par seconde.
        if dernier_affichage.elapsed() >= Duration::from_secs(1) {
            let delta = envoyes_octets - octets_precedent;
            let ecoule = dernier_affichage.elapsed().as_secs_f64();
            evenements(Evenement::Mesures(Mesures::Envoi {
                debit_mbps: delta as f64 * 8.0 / ecoule / 1e6,
                cible_mbps: pacer.target_bps() as f64 / 1e6,
                images_sautees,
                rtt_ms: dernier_rtt_ms,
            }));
            octets_precedent = envoyes_octets;
            dernier_affichage = Instant::now();
        }
    }

    let duree_s = t0.elapsed().as_secs_f64();
    echantillons_rtt.sort_by(|a, b| a.partial_cmp(b).unwrap());
    echantillons_encode_us.sort_unstable();
    let encodage_ms = (!echantillons_encode_us.is_empty()).then(|| Quantiles {
        p50: percentile_u64(&echantillons_encode_us, 50) as f64 / 1000.0,
        p99: percentile_u64(&echantillons_encode_us, 99) as f64 / 1000.0,
        echantillons: echantillons_encode_us.len(),
    });
    let rtt_ms = (!echantillons_rtt.is_empty()).then(|| Quantiles {
        p50: percentile_f64(&echantillons_rtt, 50),
        p99: percentile_f64(&echantillons_rtt, 99),
        echantillons: echantillons_rtt.len(),
    });
    let (_, vers_internet) = link.destinations();
    Ok(Fin::DureeEcoulee(Box::new(Bilan::Envoi(BilanEnvoi {
        duree_s,
        images_encodees,
        images_sautees,
        encodage_ms,
        envoyes_octets,
        cible_finale_bps: pacer.target_bps(),
        retours,
        echecs_envoi: echecs_send,
        tentatives_envoi: tentatives_send,
        rtt_ms,
        vers_internet,
    }))))
}

// … ici, recopiées de cmd_host.rs (C2) lignes 517-642, à l'identique :
// servir_reseau, ResultatEnvoi, envoyer_ou_abandonner, percentile_f64,
// percentile_u64.
```

Les deux blocs « recopiées à l'identique » sont un **déplacement** : copier les lignes indiquées de
l'ancien `cmd_host.rs` (`git show HEAD:spike/crates/sky-probe/src/cmd_host.rs`), commentaires
compris, sans en changer un caractère hormis `pub(crate)`/`const` → `pub` comme indiqué.

Ajouter à la fin de `hote.rs` le test de l'arrêt pendant l'attente :

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn un_arret_pendant_l_attente_termine_heberger_sans_attendre_la_fenetre() {
        // La fenêtre dure 30 minutes : sans l'horloge arrêtable, ce test ne
        // rendrait jamais la main — d'où le fil et le `recv_timeout`.
        // Neutralisation : `HorlogeReelle::demarrer()` au lieu de
        // `HorlogeArretable::demarrer(arret)` — rouge au délai de 20 s.
        let (envoi, reception) = mpsc::channel();
        std::thread::spawn(move || {
            let coffre = Coffre::pour_test("sky-test-partage-arret-hote");
            coffre.ranger_identifiant_appareil(1).unwrap();
            let config = Config::vers("http://127.0.0.1:1");
            let arret = Arret::nouveau();
            let mut appels = 0u32;
            let mut evenements = Vec::new();
            let fin = heberger(
                &config,
                &coffre,
                |_| {
                    appels += 1;
                    if appels == 2 {
                        arret.demander();
                    }
                    Ok(Etat {
                        version: 1,
                        code: "ABCDEFGH".to_string(),
                        amis: Vec::new(),
                        demandes: Vec::new(),
                        listes: Vec::new(),
                        appareils: Vec::new(),
                        enveloppes: Vec::new(),
                    })
                },
                ParametresHote {
                    codec: Codec::Hevc444,
                    plafond_mbps: 30,
                    plancher_mbps: 10,
                    moniteur: 0,
                    source: SourceImages::Ecran,
                    duree_max: None,
                },
                &arret,
                &mut |e| evenements.push(e),
            );
            let _ = envoi.send((fin.map_err(|e| format!("{e:?}")), appels, evenements));
        });
        let (fin, appels, evenements) =
            reception.recv_timeout(Duration::from_secs(20)).expect("heberger n'a pas rendu la main en 20 s");
        assert_eq!(fin, Ok(Fin::Arrete));
        assert_eq!(appels, 2, "aucune synchronisation après l'arrêt");
        assert_eq!(evenements, vec![Evenement::Pret, Evenement::Disponible { fenetre: FENETRE_HOTE }]);
    }
}
```

(`arret` est emprunté par la fermeture de synchronisation **et** passé à `heberger` : les deux
emprunts sont partagés, `Arret::demander` prend `&self`. Si le compilateur refuse la capture, cloner :
`let arret_fermeture = arret.clone();` et l'utiliser dans la fermeture.)

- [ ] **Étape 8 : implémenter `spectateur.rs`** (au-dessus du module de tests)

```rust
//! Côté spectateur : demander le partage d'un ami, intégrer sa réponse, puis
//! recevoir et MESURER le flux. Déplacé de `sky-probe/src/cmd_view.rs` (C2).
//! Le flux est JETÉ par défaut (spec D2) : seul un appelant qui fournit un
//! puits (`sky-probe view` et son fichier) en garde les octets.

use std::io::Write;
use std::time::{Duration, Instant};

use sky_compte::{deposer, relever, resoudre_ami, Ami, Coffre, Config, ErreurCompte, Etat};
use sky_crypto::Identity;
use sky_net::{LinkEvent, PeerLink};

use crate::arret::{synchroniser_sauf_arret, Arret, ErreurAttente, HorlogeArretable};
use crate::etablissement::{etablir, Etablissement};
use crate::evenement::{Bilan, BilanReception, ErreurPartage, Evenement, Fin, Mesures};
use crate::hote::epoch_us;
use crate::reception::Reception;
use crate::rendez_vous::{interroger, reponse_a_l_offre, session_de, ATTENTE_SPECTATEUR, CADENCE};

/// Période d'émission du retour vers l'hôte (C2) : l'horodatage du dernier
/// paquet reçu, dont l'hôte tire un RTT réel pour son `Pacer`.
const PERIODE_RETOUR: Duration = Duration::from_millis(200);

/// Où écrire le flux reçu, si quelqu'un le veut.
pub type Puits = Box<dyn Write>;

/// Comment l'appelant désigne l'ami à regarder.
pub enum Designation<'a> {
    /// Nom Discord exact ou identifiant écrit en texte (`sky-probe view`) :
    /// passe par `resoudre_ami`, qui refuse toute ambiguïté.
    Texte(&'a str),
    /// Identifiant d'utilisateur (l'application) : jamais confondu avec un nom.
    Identifiant(i64),
}

pub fn trouver_ami<'e>(etat: &'e Etat, designation: &Designation<'_>) -> Result<&'e Ami, ErreurCompte> {
    match designation {
        Designation::Texte(texte) => resoudre_ami(etat, texte),
        Designation::Identifiant(id) => etat
            .amis
            .iter()
            .find(|ami| ami.id == *id)
            .ok_or_else(|| ErreurCompte::Protocole(format!("aucun ami d'identifiant {id}"))),
    }
}

pub struct ParametresSpectateur<'a> {
    pub ami: Designation<'a>,
    /// `None` : jusqu'à l'arrêt (application). `Some` : `sky-probe --seconds`.
    pub duree_max: Option<Duration>,
}

pub fn regarder(
    config: &Config,
    coffre: &Coffre,
    mut synchroniser: impl FnMut(Option<&Etat>) -> Result<Etat, ErreurCompte>,
    p: ParametresSpectateur<'_>,
    ouvrir_puits: impl FnOnce() -> anyhow::Result<Option<Puits>>,
    arret: &Arret,
    evenements: &mut dyn FnMut(Evenement),
) -> Result<Fin, ErreurPartage> {
    let lancement = Instant::now();
    // Avant tout réseau : sans appareil, `deposer` refuserait de toute façon.
    if coffre.identifiant_appareil().map_err(ErreurPartage::Compte)?.is_none() {
        return Ok(Fin::AucunAppareilLocal);
    }
    // L'identité DURABLE : c'est pour sa clé d'annuaire que l'hôte scelle
    // l'enveloppe de sa réponse. La clé de l'offre, elle, est éphémère.
    let identite = coffre.identite().map_err(ErreurPartage::Compte)?;

    let etat = synchroniser(None).map_err(ErreurPartage::Compte)?;
    let ami = trouver_ami(&etat, &p.ami).map_err(ErreurPartage::Compte)?.clone();
    if ami.appareils.is_empty() {
        return Ok(Fin::AucunAppareilChezLAmi { nom: ami.discord_name });
    }

    let (mut link, offre) = PeerLink::offrant(Identity::generate())?;
    let session = session_de(&offre)?;
    let deposes = deposer(config, coffre, &ami.appareils, offre.as_bytes()).map_err(ErreurPartage::Compte)?;
    if deposes == 0 {
        return Ok(Fin::DemandeRefusee { nom: ami.discord_name, appareils: ami.appareils.len() });
    }
    evenements(Evenement::DemandeEnvoyee {
        nom: ami.discord_name.clone(),
        deposes,
        appareils: ami.appareils.len(),
        attente: ATTENTE_SPECTATEUR,
    });

    // Le mapping NAT du port annoncé dans l'offre doit survivre à l'attente.
    let garde = link.maintenir_mapping()?;
    let mut synchronisations = 1u32; // celle qui a résolu l'ami
    let mut horloge = HorlogeArretable::demarrer(arret);
    let attente = interroger(
        Some(etat),
        synchroniser_sauf_arret(arret, |precedent| {
            synchronisations += 1;
            synchroniser(precedent)
        }),
        |etat| reponse_a_l_offre(relever(etat, &identite), session, &ami.appareils),
        &mut horloge,
        CADENCE,
        ATTENTE_SPECTATEUR,
    );
    let reponse = match attente {
        Ok(Some(reponse)) => reponse,
        Ok(None) => return Ok(Fin::PasDeReponse { nom: ami.discord_name }),
        Err(ErreurAttente::Arrete) => return Ok(Fin::Arrete),
        Err(ErreurAttente::Compte(e)) => return Err(ErreurPartage::Compte(e)),
    };
    evenements(Evenement::ReponseRecue { apres: lancement.elapsed(), synchronisations });

    // La négociation produit désormais son propre trafic.
    drop(garde);
    link.accepter_reponse(&reponse)?;
    evenements(Evenement::Negociation);

    let duree = match etablir(&mut link, arret)? {
        Etablissement::Ouvert(duree) => duree,
        Etablissement::Rompu(raison) => return Ok(Fin::NegociationRompue(raison)),
        Etablissement::Delai(diagnostic) => return Ok(Fin::EtablissementEchoue(diagnostic)),
        Etablissement::Arrete => return Ok(Fin::Arrete),
    };
    evenements(Evenement::Connecte { en: duree, depuis_le_lancement: Some(lancement.elapsed()) });

    // Ouvert APRÈS la connexion, comme le fichier de `view` au C2 : jamais de
    // fichier vide laissé par une négociation ratée.
    let puits = ouvrir_puits()?;
    recevoir(&mut link, puits, p.duree_max, arret, evenements)
}

/// La boucle de réception de `cmd_view` (C2), l'écriture du fichier devenue
/// optionnelle. Les en-têtes de séquence n'étant émis qu'une fois (GOP
/// infini), le puits reçoit tout depuis le tout premier paquet.
fn recevoir(
    link: &mut PeerLink,
    mut puits: Option<Puits>,
    duree_max: Option<Duration>,
    arret: &Arret,
    evenements: &mut dyn FnMut(Evenement),
) -> Result<Fin, ErreurPartage> {
    let mut reception = Reception::default();
    let t0 = Instant::now();
    let mut dernier_affichage = Instant::now();
    let mut dernier_retour = Instant::now();
    let mut octets_precedent = 0u64;
    let mut images_precedent = 0u64;

    loop {
        if arret.est_demande() {
            vider(&mut puits);
            return Ok(Fin::Arrete);
        }
        if duree_max.is_some_and(|d| t0.elapsed() >= d) {
            break;
        }
        match link.poll()? {
            LinkEvent::Data(d) => {
                if let Some(charge) = reception.absorber(&d, epoch_us()) {
                    if let Some(p) = puits.as_mut() {
                        p.write_all(charge).map_err(anyhow::Error::from)?;
                    }
                }
            }
            LinkEvent::Failed(raison) => {
                vider(&mut puits);
                return Ok(Fin::LienTombe(raison));
            }
            _ => {}
        }

        if dernier_retour.elapsed() >= PERIODE_RETOUR {
            if let Some(h) = reception.dernier_horodatage_emission() {
                let _ = link.send(&h.to_le_bytes());
            }
            dernier_retour = Instant::now();
        }

        if dernier_affichage.elapsed() >= Duration::from_secs(1) {
            let ecoule = dernier_affichage.elapsed().as_secs_f64();
            evenements(Evenement::Mesures(Mesures::Reception {
                debit_mbps: (reception.octets() - octets_precedent) as f64 * 8.0 / ecoule / 1e6,
                images_par_s: reception.images() - images_precedent,
                gigue_ms: reception.gigue_ms(),
            }));
            octets_precedent = reception.octets();
            images_precedent = reception.images();
            dernier_affichage = Instant::now();
        }
    }

    if let Some(p) = puits.as_mut() {
        p.flush().map_err(anyhow::Error::from)?;
    }
    let (_, vers_internet) = link.destinations();
    Ok(Fin::DureeEcoulee(Box::new(Bilan::Reception(BilanReception {
        duree_s: t0.elapsed().as_secs_f64(),
        images: reception.images(),
        octets: reception.octets(),
        transit_ms: reception.transit_ms(),
        gigue_ms: reception.gigue_ms(),
        vers_internet,
    }))))
}

fn vider(puits: &mut Option<Puits>) {
    if let Some(p) = puits.as_mut() {
        p.flush().ok();
    }
}
```

`spike/crates/sky-partage/src/lib.rs` devient :

```rust
//! `sky-partage` : la négociation d'une connexion par la boîte aux lettres,
//! puis la diffusion et la réception (jalon 1, décision D5 de la spec).
//! Utilisée par `sky-probe` (affichage terminal) et par l'application
//! (`sky-app`). Elle ne fait AUCUNE sortie terminal : elle rend des
//! événements typés et accepte un signal d'arrêt.

pub mod arret;
pub mod etablissement;
pub mod evenement;
pub mod hote;
pub mod reception;
pub mod rendez_vous;
pub mod spectateur;

pub use arret::{synchroniser_sauf_arret, Arret, ErreurAttente, HorlogeArretable, PAS_D_ATTENTE};
pub use evenement::{
    Bilan, BilanEnvoi, BilanReception, Diagnostic, ErreurPartage, Evenement, Fin, Mesures, Quantiles,
};
pub use hote::{heberger, FabriqueSynthetique, Images, ParametresHote, SourceImages, FPS};
pub use spectateur::{regarder, trouver_ami, Designation, ParametresSpectateur, Puits};
```

- [ ] **Étape 9 : lancer les tests de `sky-partage`**

```bash
cd spike && cargo test -p sky-partage
```
Attendu : PASS — 20 (T4) + 4 + 1 + 1 = **26**.

- [ ] **Étape 10 : réécrire `sky-probe host` en affichage**

`spike/crates/sky-probe/src/cmd_encode.rs:33` devient :
```rust
pub(crate) const FPS: u32 = sky_partage::FPS;
```

Remplacer tout le contenu de `spike/crates/sky-probe/src/cmd_host.rs` par :

```rust
//! `sky-probe host` : un affichage de `sky_partage::heberger` (jalon 1,
//! tâche 5). La négociation et la diffusion vivent dans `sky-partage` ; ce
//! fichier ne garde que les textes du terminal, inchangés depuis le C2.

use std::io::Write;
use std::time::Duration;

use sky_compte::{synchroniser, ErreurCompte};
use sky_encode::Codec;
use sky_partage::hote::BUDGET_RETRY_ENVOI;
use sky_partage::rendez_vous::FENETRE_HOTE;
use sky_partage::{
    heberger, Arret, Bilan, BilanEnvoi, Diagnostic, ErreurPartage, Evenement, Fin, Images, Mesures,
    ParametresHote, SourceImages,
};
use windows::Win32::Graphics::Direct3D11::ID3D11Device;

use crate::cmd_compte::{avertissement_consommation, causes_d_un_depot_refuse, config_et_coffre, message_utilisateur};
use crate::cmd_encode::{Source, TextureSynthetique};

/// Paramètres de la chaîne complète (inchangés depuis le C2).
pub struct Parametres {
    pub secondes: u64,
    pub codec: Codec,
    pub bitrate_mbps: u32,
    pub floor_mbps: u32,
    pub monitor: usize,
    pub source: Source,
    pub largeur_synth: u32,
    pub hauteur_synth: u32,
}

impl Images for TextureSynthetique {
    fn prochaine_image(&mut self) -> anyhow::Result<sky_capture::CapturedFrame> {
        TextureSynthetique::prochaine_image(self)
    }
}

fn fabrique_synthetique(device: &ID3D11Device, largeur: u32, hauteur: u32) -> anyhow::Result<Box<dyn Images>> {
    Ok(Box::new(TextureSynthetique::new(device, largeur, hauteur)?))
}

pub fn run(p: Parametres) -> anyhow::Result<()> {
    let (config, coffre) = config_et_coffre()?;
    let source = match p.source {
        Source::Ecran => SourceImages::Ecran,
        Source::Synthetique => SourceImages::Synthetique {
            largeur: p.largeur_synth,
            hauteur: p.hauteur_synth,
            fabrique: fabrique_synthetique,
        },
    };
    let parametres = ParametresHote {
        codec: p.codec,
        plafond_mbps: p.bitrate_mbps,
        plancher_mbps: p.floor_mbps,
        moniteur: p.monitor,
        source,
        duree_max: Some(Duration::from_secs(p.secondes)),
    };
    // Jamais demandé : `host` s'arrête à `--seconds`, comme au C2.
    let arret = Arret::nouveau();
    let fin = heberger(
        &config,
        &coffre,
        |precedent| synchroniser(&config, &coffre, precedent),
        parametres,
        &arret,
        &mut |evenement| afficher(&lignes_hote(&evenement)),
    )
    .map_err(erreur_partage)?;
    afficher_fin(fin, p.floor_mbps, p.bitrate_mbps)
}

pub(crate) fn afficher(lignes: &[String]) {
    for ligne in lignes {
        println!("{ligne}");
    }
    std::io::stdout().flush().ok();
}

/// Les lignes du C2, pour chaque événement de l'hôte.
pub(crate) fn lignes_hote(evenement: &Evenement) -> Vec<String> {
    match evenement {
        Evenement::Pret => vec![avertissement_consommation("la demande de ton ami")],
        Evenement::Disponible { fenetre } => vec![format!(
            "En attente de la demande d'un ami, pendant {} minutes au maximum...",
            fenetre.as_secs() / 60
        )],
        Evenement::EchecLocal { raison } => vec![message_echec_local(raison)],
        Evenement::DemandeEcartee { raison } => vec![format!("  Demande écartée : {raison}")],
        Evenement::DemandeRecue { apres, synchronisations, .. } => vec![format!(
            "Demande reçue après {} s ({synchronisations} synchronisations).",
            apres.as_secs()
        )],
        Evenement::Negociation => vec!["Réponse envoyée. Négociation en cours...".to_string()],
        Evenement::Connecte { en, .. } => vec![format!("CONNECTÉ en {:.1} s", en.as_secs_f32())],
        Evenement::Diffusion { largeur, hauteur, codec, plancher_mbps, plafond_mbps } => vec![format!(
            "\nRésolution {largeur}x{hauteur}, {}, plancher {plancher_mbps} Mbps, plafond {plafond_mbps} Mbps.\n",
            codec.label()
        )],
        Evenement::Mesures(Mesures::Envoi { debit_mbps, cible_mbps, images_sautees, rtt_ms }) => vec![format!(
            "  {debit_mbps:.1} Mbps envoyés | cible pacer {cible_mbps:.1} Mbps | {images_sautees} images sautées cumulées | RTT {rtt_ms:.1} ms"
        )],
        // Événements du spectateur : `heberger` ne les émet jamais.
        Evenement::DemandeEnvoyee { .. } | Evenement::ReponseRecue { .. } | Evenement::Mesures(Mesures::Reception { .. }) => {
            Vec::new()
        }
    }
}

fn afficher_fin(fin: Fin, plancher: u32, plafond: u32) -> anyhow::Result<()> {
    match fin {
        Fin::AucunAppareilLocal => anyhow::bail!(
            "aucun appareil enregistré sur cette machine — lance d'abord \
             `sky-probe device register <nom>`."
        ),
        Fin::AucuneDemande => println!(
            "Aucune demande reçue en {} minutes. Relance `sky-probe host` quand ton ami est prêt.",
            FENETRE_HOTE.as_secs() / 60
        ),
        Fin::ReponseRefusee => {
            println!("Le serveur a refusé la réponse : rien n'a été envoyé.");
            println!("{}", causes_d_un_depot_refuse());
        }
        Fin::NegociationRompue(raison) => println!("ÉCHEC : {raison}"),
        Fin::EtablissementEchoue(diagnostic) => afficher_diagnostic(&diagnostic),
        Fin::LienTombe(raison) => println!("\nÉCHEC : {raison}"),
        Fin::TamponSature { morceau, morceaux } => println!(
            "\nÉCHEC : tampon d'émission saturé plus de {} ms \
             (morceau {morceau}/{morceaux}) — arrêt pour ne pas \
             produire un flux corrompu.",
            BUDGET_RETRY_ENVOI.as_millis(),
        ),
        Fin::DureeEcoulee(bilan) => {
            if let Bilan::Envoi(b) = *bilan {
                afficher_bilan(&b, plancher, plafond);
            }
        }
        // `host` ne demande jamais l'arrêt ; les autres fins sont celles du spectateur.
        Fin::Arrete | Fin::AucunAppareilChezLAmi { .. } | Fin::DemandeRefusee { .. } | Fin::PasDeReponse { .. } => {}
    }
    Ok(())
}
```

Puis, dans le même fichier, **recopier à l'identique** du C2 : le résumé (ancien `run`, lignes
465-513) sous la forme d'une fonction `fn afficher_bilan(b: &BilanEnvoi, plancher: u32, plafond: u32)`
dont chaque `println!` lit le champ correspondant (`ecoule` → `b.duree_s` ; `images_encodees` →
`b.images_encodees` ; le bloc encodage devient `if let Some(q) = &b.encodage_ms { println!("Encodage médian/p99: {:.2} ms / {:.2} ms  ({} échantillons)", q.p50, q.p99, q.echantillons); }` ;
`envoyes_octets` → `b.envoyes_octets` ; `pacer.target_bps()` → `b.cible_finale_bps` ;
`p.floor_mbps`/`p.bitrate_mbps` → `plancher`/`plafond` ; `retours`, `echecs_send`,
`tentatives_send` → `b.retours`, `b.echecs_envoi`, `b.tentatives_envoi` ; le bloc RTT devient un
`match &b.rtt_ms { None => …, Some(q) => … }` avec les mêmes textes ; `vers_internet` →
`b.vers_internet`) ; `diagnostiquer` (lignes 706-759) sous la forme
`pub(crate) fn afficher_diagnostic(d: &Diagnostic)` (`DELAI_ETABLISSEMENT.as_secs()` → `d.delai.as_secs()`,
`link.is_connected()` → `d.ice_connecte`, `link.trafic()` → `(d.emis, d.recus, d.erreurs)`,
`link.destinations()` → `(d.vers_local, d.vers_internet)`, `link.erreurs_socket()` →
`d.erreurs_socket`) ; `message_echec_local` et `erreur_compte` (lignes 652-664) à l'identique ; et
le module `tests` existant (lignes 761-778) à l'identique. Ajouter :

```rust
/// `ErreurPartage` → l'erreur que `host`/`view` rendaient au C2.
pub(crate) fn erreur_partage(e: ErreurPartage) -> anyhow::Error {
    match e {
        ErreurPartage::Compte(c) => erreur_compte(c),
        ErreurPartage::Autre(a) => a,
    }
}
```

Et, dans le module `tests` de `cmd_host.rs`, ces deux tests (les textes attendus sont **copiés de
l'ancien code**, pas réécrits) :

```rust
    #[test]
    fn la_ligne_de_mesure_de_l_hote_est_celle_du_c2() {
        let lignes = lignes_hote(&Evenement::Mesures(Mesures::Envoi {
            debit_mbps: 12.34,
            cible_mbps: 20.0,
            images_sautees: 3,
            rtt_ms: 85.24,
        }));
        assert_eq!(
            lignes,
            vec!["  12.3 Mbps envoyés | cible pacer 20.0 Mbps | 3 images sautées cumulées | RTT 85.2 ms".to_string()]
        );
    }

    #[test]
    fn la_demande_recue_affiche_des_secondes_entieres() {
        // C2 : `debut_attente.elapsed().as_secs()`, pas une décimale.
        let lignes = lignes_hote(&Evenement::DemandeRecue {
            expediteur_device_id: 4,
            apres: Duration::from_millis(12_900),
            synchronisations: 7,
        });
        assert_eq!(lignes, vec!["Demande reçue après 12 s (7 synchronisations).".to_string()]);
    }
```

(Valeurs d'entrée choisies loin de tout cas d'arrondi à égalité : l'objet du test est le gabarit
de la ligne, pas l'arrondi.)

- [ ] **Étape 11 : réécrire `sky-probe view` en affichage**

Remplacer tout le contenu de `spike/crates/sky-probe/src/cmd_view.rs` par :

```rust
//! `sky-probe view` : un affichage de `sky_partage::regarder` (jalon 1,
//! tâche 5). Le flux reçu est écrit dans `sortie` dès le premier paquet —
//! `sky-partage` le jetterait ; c'est ce fichier qui lui fournit le puits.

use std::fs::File;
use std::time::Duration;

use sky_compte::synchroniser;
use sky_partage::{
    regarder, Arret, Bilan, BilanReception, Designation, Evenement, Fin, Mesures, ParametresSpectateur, Puits,
};

use crate::cmd_compte::{avertissement_consommation, causes_d_un_depot_refuse, config_et_coffre};
use crate::cmd_host::{afficher, afficher_diagnostic, erreur_partage};

pub fn run(ami_designe: &str, secondes: u64, sortie: &str) -> anyhow::Result<()> {
    let (config, coffre) = config_et_coffre()?;
    // Jamais demandé : `view` s'arrête à `--seconds`, comme au C2.
    let arret = Arret::nouveau();
    let fin = regarder(
        &config,
        &coffre,
        |precedent| synchroniser(&config, &coffre, precedent),
        ParametresSpectateur {
            ami: Designation::Texte(ami_designe),
            duree_max: Some(Duration::from_secs(secondes)),
        },
        || Ok(Some(Box::new(File::create(sortie)?) as Puits)),
        &arret,
        &mut |evenement| afficher(&lignes_spectateur(&evenement, sortie)),
    )
    .map_err(erreur_partage)?;
    afficher_fin(fin, sortie)
}

/// Les lignes du C2, pour chaque événement du spectateur.
fn lignes_spectateur(evenement: &Evenement, sortie: &str) -> Vec<String> {
    match evenement {
        Evenement::DemandeEnvoyee { nom, deposes, appareils, attente } => vec![
            format!("\nDemande envoyée à {nom} ({deposes} appareil(s) sur {appareils})."),
            format!("J'attends sa réponse pendant {} s au maximum.", attente.as_secs()),
            avertissement_consommation("la réponse de ton ami"),
        ],
        Evenement::ReponseRecue { apres, synchronisations } => vec![format!(
            "Réponse reçue après {:.1} s ({synchronisations} synchronisations).",
            apres.as_secs_f32()
        )],
        Evenement::Negociation => vec!["Négociation en cours...".to_string()],
        Evenement::Connecte { en, depuis_le_lancement } => vec![
            format!(
                "CONNECTÉ en {:.1} s ({:.1} s depuis le lancement de view)",
                en.as_secs_f32(),
                depuis_le_lancement.unwrap_or_default().as_secs_f32()
            ),
            format!("Écriture du flux reçu dans {sortie}, dès le premier paquet.\n"),
        ],
        Evenement::Mesures(Mesures::Reception { debit_mbps, images_par_s, gigue_ms }) => {
            vec![format!("  {debit_mbps:.1} Mbps | {images_par_s} images/s | gigue {gigue_ms:.2} ms")]
        }
        // Événements de l'hôte : `regarder` ne les émet jamais.
        Evenement::Pret
        | Evenement::Disponible { .. }
        | Evenement::DemandeEcartee { .. }
        | Evenement::EchecLocal { .. }
        | Evenement::DemandeRecue { .. }
        | Evenement::Diffusion { .. }
        | Evenement::Mesures(Mesures::Envoi { .. }) => Vec::new(),
    }
}

fn afficher_fin(fin: Fin, sortie: &str) -> anyhow::Result<()> {
    match fin {
        Fin::AucunAppareilLocal => anyhow::bail!(
            "aucun appareil enregistré sur cette machine — lance d'abord \
             `sky-probe device register <nom>`."
        ),
        Fin::AucunAppareilChezLAmi { nom } => {
            println!("{nom} n'a aucun appareil enregistré : personne à qui envoyer la demande.")
        }
        Fin::DemandeRefusee { nom, appareils } => {
            println!("Le serveur a refusé la demande pour les {appareils} appareil(s) de {nom} : rien n'a été envoyé.");
            println!("{}", causes_d_un_depot_refuse());
        }
        Fin::PasDeReponse { nom } => println!("{nom} n'a pas répondu — est-il en partage ?"),
        Fin::NegociationRompue(raison) => println!("ÉCHEC : {raison}"),
        Fin::EtablissementEchoue(diagnostic) => afficher_diagnostic(&diagnostic),
        Fin::LienTombe(raison) => println!("\nÉCHEC : {raison}"),
        Fin::DureeEcoulee(bilan) => {
            if let Bilan::Reception(b) = *bilan {
                afficher_bilan(&b, sortie);
            }
        }
        // `view` ne demande jamais l'arrêt ; les autres fins sont celles de l'hôte.
        Fin::Arrete | Fin::AucuneDemande | Fin::ReponseRefusee | Fin::TamponSature { .. } => {}
    }
    Ok(())
}

fn afficher_bilan(b: &BilanReception, sortie: &str) {
    println!(
        "\nDébit moyen reçu : {:.1} Mbps sur {:.0} s ({} images, {} Mo)",
        b.octets as f64 * 8.0 / b.duree_s / 1e6,
        b.duree_s,
        b.images,
        b.octets / 1_000_000
    );
    println!("Fichier écrit    : {sortie}");
    match &b.transit_ms {
        None => println!("Transit sur le lien : non mesuré (aucune image reçue)"),
        Some(q) => println!(
            "Transit sur le lien (médian / p99) : {:.2} ms / {:.2} ms  ({} échantillons)",
            q.p50, q.p99, q.echantillons
        ),
    }
    println!("Gigue finale (RFC 3550, lissée) : {:.2} ms", b.gigue_ms);
    if b.vers_internet == 0 {
        println!("Rappel : aucun paquet n'est parti vers internet — lien local.");
    } else {
        println!("Lien reseau reel : {} paquets emis vers internet.", b.vers_internet);
        println!("Ces mesures sont celles d'une vraie liaison entre deux machines.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_ligne_connecte_du_spectateur_est_celle_du_c2() {
        let lignes = lignes_spectateur(
            &Evenement::Connecte {
                en: Duration::from_millis(600),
                depuis_le_lancement: Some(Duration::from_millis(7_100)),
            },
            "recu.h265",
        );
        assert_eq!(
            lignes,
            vec![
                "CONNECTÉ en 0.6 s (7.1 s depuis le lancement de view)".to_string(),
                "Écriture du flux reçu dans recu.h265, dès le premier paquet.\n".to_string(),
            ]
        );
    }
}
```

Dans `cmd_host.rs`, `afficher_diagnostic` doit être `pub(crate)`.

- [ ] **Étape 12 : lancer tout**

```bash
cd spike && cargo test && cargo clippy --all-targets -- -D warnings && cargo run -p sky-probe -- selftest
```
Attendu : PASS ; `sky-probe` : **23** tests (20 + 3) ; `sky-partage` : **26** ; `selftest` négocie
et ouvre le canal comme avant ; `grep -rn "stdin" spike/crates/sky-probe/src/cmd_host.rs spike/crates/sky-probe/src/cmd_view.rs` : aucune ligne.

- [ ] **Étape 13 : prouver par neutralisation (une à la fois)**

Les quatre tests de `reception.rs`, celui de `spectateur.rs` et celui de `hote.rs`, selon leurs
commentaires. Pour les trois tests de textes de `sky-probe` : remplacer dans `lignes_hote` le
`{rtt_ms:.1}` par `{rtt_ms:.2}` — `la_ligne_de_mesure…` rougit ; remplacer `apres.as_secs()` par
`apres.as_secs_f32()` — `la_demande_recue…` rougit ; retirer le `\n` final de la ligne « Écriture du
flux » — `la_ligne_connecte…` rougit.

- [ ] **Étape 14 : relecture des textes, ligne à ligne**

Le relecteur de cette tâche ouvre côte à côte `git show HEAD~1:spike/crates/sky-probe/src/cmd_host.rs`
et `cmd_view.rs` d'avant, et les nouveaux fichiers, et coche **chaque** `println!` de l'ancien code
contre la ligne qui la produit maintenant (`lignes_hote`, `lignes_spectateur`, `afficher_fin`,
`afficher_bilan`, `afficher_diagnostic`, `message_echec_local`). Une ligne sans équivalent, ou dont
le texte diffère d'un caractère, est un défaut de la tâche.

- [ ] **Étape 15 : commiter et pousser**

```bash
git add spike/crates/sky-partage/Cargo.toml
git add spike/crates/sky-partage/src/lib.rs
git add spike/crates/sky-partage/src/evenement.rs
git add spike/crates/sky-partage/src/etablissement.rs
git add spike/crates/sky-partage/src/reception.rs
git add spike/crates/sky-partage/src/hote.rs
git add spike/crates/sky-partage/src/spectateur.rs
git add spike/crates/sky-probe/src/cmd_host.rs
git add spike/crates/sky-probe/src/cmd_view.rs
git add spike/crates/sky-probe/src/cmd_encode.rs
git add spike/Cargo.lock
git diff --cached --stat
git commit -m "refactor: host et view deviennent des affichages de sky-partage, qui rend des evenements"
git push
```

---

## Task 6 : Squelette de l'application

**Ce que la tâche livre :** l'application compile, se lance, vit près de l'horloge, ne se ferme pas
quand on ferme sa fenêtre, refuse une seconde instance, démarre avec Windows (activé au premier
lancement de la version publiée, réglable plus tard), et s'installe par un installateur NSIS.
L'interface n'est encore qu'une disposition vide aux couleurs du site.

**Arbitrages :**
- **Démarrage automatique activé au premier lancement, en version publiée seulement**
  (`cfg(not(debug_assertions))`). Spec D4 : « vit comme Discord ». Réservé à la version publiée pour
  qu'aucun test ni `cargo run` n'écrive dans la clé `Run` du registre de la machine de développement.
  Le témoin de premier lancement est un fichier vide dans le dossier de données de l'application.
- Au démarrage automatique, l'application est lancée avec `--demarrage` et **reste cachée** (icône
  seule). Lancée à la main, elle montre sa fenêtre.
- L'icône est construite par code (`TrayIconBuilder`), pas par `tauri.conf.json` : la tâche 11 doit
  pouvoir la changer par son identifiant (`ID_ICONE`).
- CSP : `default-src 'self' ipc: http://ipc.localhost` (l'IPC de Tauri 2), styles en ligne permis
  pour Tailwind, polices servies localement. Aucune origine externe.

**Files:**
- Create: `spike/crates/sky-app/Cargo.toml`, `build.rs`, `tauri.conf.json`, `capabilities/default.json`, `icons/source.svg` (+ icônes générées), `src/main.rs`, `src/lib.rs`, `src/demarrage.rs`, `tests/instance_unique.rs`
- Create: `app/package.json`, `app/package-lock.json` (généré), `app/index.html`, `app/vite.config.ts`, `app/tsconfig.json`, `app/src/main.tsx`, `app/src/styles.css`, `app/src/Disposition.tsx`, `app/src/App.tsx`, `app/src/App.test.tsx`, `app/src/test/installation.ts`
- Modify: `.gitignore`, `spike/Cargo.lock`

**Interfaces:**
- Produces (Rust) : `sky_app_lib::lancer()` ; `pub const ID_ICONE: &str = "principal"` ;
  `demarrage::{ARGUMENT_DEMARRAGE: &str, lance_au_demarrage(arguments: impl IntoIterator<Item = String>) -> bool, premier_lancement(dossier: &Path) -> std::io::Result<bool>}` ;
  fonctions privées `montrer_fenetre(app: &AppHandle)`, `installer_icone(app: &AppHandle) -> tauri::Result<()>`.
- Produces (interface) : `export type Ecran = "amis" | "listes" | "compte"` ;
  `Disposition(props: { ecran: Ecran; choisir: (e: Ecran) => void; bas: ReactNode; children: ReactNode })` ;
  jetons Tailwind `fond`, `surface`, `surface-haute`, `bordure`, `accent`, `succes`, `alerte`, `texte`,
  `texte-2`, `texte-3`, polices `titre`, `corps`.

- [ ] **Étape 1 : l'interface — configuration**

Créer `app/package.json` (versions **exactes**, relevées en tête de ce plan) :

```json
{
  "name": "skyshare-app",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc --noEmit && vite build",
    "test": "vitest run"
  },
  "dependencies": {
    "@fontsource/instrument-serif": "5.3.0",
    "@fontsource/manrope": "5.3.0",
    "@tauri-apps/api": "2.11.1",
    "react": "19.3.0",
    "react-dom": "19.3.0"
  },
  "devDependencies": {
    "@tailwindcss/vite": "4.3.3",
    "@tauri-apps/cli": "2.11.4",
    "@testing-library/dom": "10.4.2",
    "@testing-library/jest-dom": "7.0.1",
    "@testing-library/react": "16.3.3",
    "@testing-library/user-event": "14.6.7",
    "@types/react": "19.3.0",
    "@types/react-dom": "19.3.0",
    "@vitejs/plugin-react": "6.1.1",
    "jsdom": "30.1.0",
    "tailwindcss": "4.3.3",
    "typescript": "5.9.3",
    "vite": "8.3.0",
    "vitest": "5.0.1"
  }
}
```

Créer `app/vite.config.ts` :

```ts
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Port fixe : `devUrl` de spike/crates/sky-app/tauri.conf.json le vise.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { outDir: "dist" },
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test/installation.ts"],
    css: false,
  },
});
```

Créer `app/tsconfig.json` :

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "react-jsx",
    "strict": true,
    "noEmit": true,
    "isolatedModules": true,
    "skipLibCheck": true
  },
  "include": ["src", "vite.config.ts"]
}
```

Créer `app/index.html` :

```html
<!doctype html>
<html lang="fr">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>SkyShare</title>
  </head>
  <body>
    <div id="racine"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

Créer `app/src/styles.css` — valeurs relevées dans `src/app/globals.css` du site (spec D6) :

```css
@import "tailwindcss";
@import "@fontsource/manrope/400.css";
@import "@fontsource/manrope/600.css";
@import "@fontsource/instrument-serif/400.css";

/* Direction artistique du site (src/app/globals.css), spec D6. */
@theme {
  --color-fond: #16120F;
  --color-surface: #1D1815;
  --color-surface-haute: #26201B;
  --color-bordure: #332B25;
  --color-accent: #C4664A;
  --color-succes: #6E8F6A;
  --color-alerte: #B0553F;
  --color-texte: #F2ECE6;
  --color-texte-2: #A79E96;
  --color-texte-3: #8C837B;
  --font-titre: "Instrument Serif", Georgia, serif;
  --font-corps: "Manrope", ui-sans-serif, system-ui, sans-serif;
}

body {
  margin: 0;
  background: var(--color-fond);
  color: var(--color-texte);
  font-family: var(--font-corps);
}
```

Créer `app/src/test/installation.ts` :

```ts
import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

// Sans globales Vitest, le nettoyage automatique de Testing Library ne
// s'installe pas : chaque test laisserait son rendu au suivant.
afterEach(() => cleanup());
```

Installer :
```bash
npm --prefix app install
```
Attendu : `app/package-lock.json` créé, aucune erreur de dépendance entre pairs.

- [ ] **Étape 2 : écrire le test de disposition qui échoue**

Créer `app/src/App.test.tsx` :

```tsx
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "./App";

describe("disposition", () => {
  it("la barre latérale mène aux trois écrans et garde le partage en bas", async () => {
    // Spec D6. Neutralisation : retirer l'entrée « Mon compte » de
    // Disposition — le test rougit sur cette entrée.
    render(<App />);
    const navigation = screen.getByRole("navigation", { name: "Navigation" });
    for (const nom of ["Amis", "Listes", "Mon compte"]) {
      expect(within(navigation).getByRole("button", { name: nom })).toBeInTheDocument();
    }
    expect(within(navigation).getByRole("button", { name: "Partager mon écran" })).toBeInTheDocument();
    await userEvent.click(within(navigation).getByRole("button", { name: "Listes" }));
    expect(screen.getByRole("heading", { name: "Listes" })).toBeInTheDocument();
  });
});
```

```bash
npm --prefix app test
```
Attendu : FAIL — `./App` introuvable.

- [ ] **Étape 3 : écrire la disposition**

`app/src/Disposition.tsx` :

```tsx
import type { ReactNode } from "react";

export type Ecran = "amis" | "listes" | "compte";

const ENTREES: { ecran: Ecran; libelle: string }[] = [
  { ecran: "amis", libelle: "Amis" },
  { ecran: "listes", libelle: "Listes" },
  { ecran: "compte", libelle: "Mon compte" },
];

/** Barre latérale à gauche, bouton de partage toujours visible en bas (spec D6). */
export function Disposition(props: {
  ecran: Ecran;
  choisir: (ecran: Ecran) => void;
  bas: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="flex h-screen bg-fond font-corps text-texte">
      <nav aria-label="Navigation" className="flex w-60 flex-col border-r border-bordure bg-surface p-4">
        <p className="mb-6 font-titre text-3xl">SkyShare</p>
        <ul className="flex flex-col gap-1">
          {ENTREES.map((entree) => (
            <li key={entree.ecran}>
              <button
                type="button"
                aria-current={props.ecran === entree.ecran ? "page" : undefined}
                className={`w-full rounded-md px-3 py-2 text-left ${
                  props.ecran === entree.ecran ? "bg-surface-haute text-texte" : "text-texte-2 hover:bg-surface-haute"
                }`}
                onClick={() => props.choisir(entree.ecran)}
              >
                {entree.libelle}
              </button>
            </li>
          ))}
        </ul>
        <div className="mt-auto">{props.bas}</div>
      </nav>
      <main className="flex-1 overflow-y-auto p-8">{props.children}</main>
    </div>
  );
}
```

`app/src/App.tsx` :

```tsx
import { useState } from "react";
import { Disposition, type Ecran } from "./Disposition";

const TITRES: Record<Ecran, string> = { amis: "Amis", listes: "Listes", compte: "Mon compte" };

export function App() {
  const [ecran, setEcran] = useState<Ecran>("amis");
  return (
    <Disposition
      ecran={ecran}
      choisir={setEcran}
      bas={
        <button type="button" disabled className="w-full rounded-md bg-accent px-3 py-2 text-fond disabled:opacity-50">
          Partager mon écran
        </button>
      }
    >
      <h1 className="font-titre text-4xl">{TITRES[ecran]}</h1>
    </Disposition>
  );
}
```

`app/src/main.tsx` :

```tsx
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./styles.css";
import { App } from "./App";

createRoot(document.getElementById("racine")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
```

```bash
npm --prefix app test && npm --prefix app run build
```
Attendu : 1 test vert ; `app/dist/index.html` produit. Neutraliser comme écrit dans le test ;
rétablir.

- [ ] **Étape 4 : la crate — manifeste, construction, configuration**

`spike/crates/sky-app/Cargo.toml` :

```toml
[package]
name = "sky-app"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
publish.workspace = true

# Le suffixe `_lib` évite, sous Windows, une collision de fichiers entre la
# bibliothèque et l'exécutable de même nom (cargo#8519) — convention du
# modèle Tauri 2. Le cœur vit dans la bibliothèque pour être testable.
[lib]
name = "sky_app_lib"
path = "src/lib.rs"

[[bin]]
name = "sky-app"
path = "src/main.rs"

[build-dependencies]
tauri-build = { version = "=2.6.3", features = [] }

[dependencies]
# `tray-icon` : l'icône près de l'horloge ; `image-png` : `include_image!`.
tauri = { version = "=2.11.5", features = ["tray-icon", "image-png"] }
tauri-plugin-single-instance = "=2.4.4"
tauri-plugin-autostart = "=2.5.1"
serde.workspace = true
serde_json.workspace = true
```

`spike/crates/sky-app/build.rs` :

```rust
fn main() {
    tauri_build::build()
}
```

`spike/crates/sky-app/tauri.conf.json` :

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "SkyShare",
  "version": "0.1.0",
  "identifier": "fr.jlskyzer.skyshare",
  "build": {
    "devUrl": "http://localhost:1420",
    "frontendDist": "../../../app/dist"
  },
  "app": {
    "windows": [
      {
        "label": "main",
        "title": "SkyShare",
        "width": 1100,
        "height": 720,
        "minWidth": 900,
        "minHeight": 600,
        "visible": false
      }
    ],
    "security": {
      "csp": "default-src 'self' ipc: http://ipc.localhost; style-src 'self' 'unsafe-inline'; img-src 'self' data:"
    }
  },
  "bundle": {
    "active": true,
    "targets": ["nsis"],
    "icon": ["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png", "icons/icon.ico"]
  }
}
```

(`visible: false` : la fenêtre ne s'affiche que si l'application n'a pas été lancée au démarrage de
Windows — voir `lancer`. L'installation se fait par défaut pour l'utilisateur courant, sans droits
d'administrateur : c'est la valeur par défaut de `bundle.windows.nsis.installMode`, relevée dans
`tauri-utils` 2.9.3.)

`spike/crates/sky-app/capabilities/default.json` :

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Fenêtre principale : événements du cœur et commandes de l'application.",
  "windows": ["main"],
  "permissions": ["core:default"]
}
```

`spike/crates/sky-app/icons/source.svg` :

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024">
  <rect width="1024" height="1024" rx="224" fill="#16120F"/>
  <circle cx="512" cy="512" r="300" fill="none" stroke="#C4664A" stroke-width="96"/>
  <circle cx="512" cy="512" r="96" fill="#F2ECE6"/>
</svg>
```

Générer les icônes, puis ne garder que celles qui servent :

```bash
cd spike/crates/sky-app && ../../../app/node_modules/.bin/tauri icon icons/source.svg -o icons
ls icons
```
Attendu : `32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.ico`, `icon.png` (entre autres).
Supprimer les fichiers générés inutiles à Windows : `icon.icns`, `Square*Logo.png`,
`StoreLogo.png`, les dossiers `android/` et `ios/` s'ils existent.

Dans `.gitignore` (racine), sous `# Tauri`, ajouter :

```gitignore
spike/crates/sky-app/gen/
```

- [ ] **Étape 5 : écrire les tests qui échouent — démarrage et instance unique**

Créer `spike/crates/sky-app/src/demarrage.rs` avec seulement :

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_premier_lancement_n_est_vu_qu_une_fois() {
        // Neutralisation : ne pas écrire le témoin — le second appel rend
        // encore `true` et le démarrage automatique serait réactivé à chaque
        // lancement, contre le choix de l'utilisateur.
        let dossier = std::env::temp_dir().join(format!("skyshare-test-premier-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dossier);
        assert!(premier_lancement(&dossier).unwrap());
        assert!(!premier_lancement(&dossier).unwrap());
        std::fs::remove_dir_all(&dossier).unwrap();
    }

    #[test]
    fn seul_l_argument_de_demarrage_cache_la_fenetre() {
        assert!(lance_au_demarrage(["sky-app.exe".to_string(), ARGUMENT_DEMARRAGE.to_string()]));
        assert!(!lance_au_demarrage(["sky-app.exe".to_string()]));
    }
}
```

Créer `spike/crates/sky-app/tests/instance_unique.rs` :

```rust
//! Instance unique (spec D4) : deux instances se voleraient les enveloppes,
//! que le serveur efface en les livrant. Ce test lance le VRAI exécutable
//! deux fois — c'est le branchement de l'extension qu'il prouve, pas
//! l'extension elle-même.
//!
//! `SKY_API_URL` vise un port fermé : aucune requête n'atteint le site.
//! `--demarrage` garde la fenêtre cachée. Le démarrage automatique n'est
//! activé qu'en version publiée : ce test n'écrit pas dans le registre.

use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

fn lancer() -> Child {
    Command::new(env!("CARGO_BIN_EXE_sky-app"))
        .arg("--demarrage")
        .env("SKY_API_URL", "http://127.0.0.1:1")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("lancement de sky-app")
}

fn fin_dans(enfant: &mut Child, delai: Duration) -> Option<ExitStatus> {
    let limite = Instant::now() + delai;
    while Instant::now() < limite {
        if let Some(statut) = enfant.try_wait().expect("try_wait") {
            return Some(statut);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    None
}

#[test]
fn une_seconde_instance_rend_la_main_et_la_premiere_continue() {
    // Neutralisation : retirer `.plugin(tauri_plugin_single_instance::init(…))`
    // de `lancer()` — la seconde instance ne s'arrête pas en 15 s.
    let mut premiere = lancer();
    if let Some(statut) = fin_dans(&mut premiere, Duration::from_secs(5)) {
        panic!(
            "la première instance s'est arrêtée d'elle-même ({statut}) : une instance de SkyShare \
             tourne-t-elle déjà sur cette machine (version installée) ? La quitter, puis relancer."
        );
    }
    let mut seconde = lancer();
    let fin_seconde = fin_dans(&mut seconde, Duration::from_secs(15));
    let premiere_vivante = premiere.try_wait().expect("try_wait").is_none();

    let _ = seconde.kill();
    let _ = premiere.kill();
    let _ = seconde.wait();
    let _ = premiere.wait();

    let statut = fin_seconde.expect("la seconde instance tournait encore après 15 s : deux instances en même temps");
    assert!(statut.success(), "la seconde instance doit rendre la main proprement ({statut})");
    assert!(premiere_vivante, "la première instance doit continuer de tourner");
}
```

- [ ] **Étape 6 : écrire `main.rs`, `lib.rs` SANS l'instance unique, et voir le test rougir**

`spike/crates/sky-app/src/main.rs` :

```rust
// Pas de console en version publiée : la fenêtre et l'icône suffisent.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    sky_app_lib::lancer();
}
```

`spike/crates/sky-app/src/lib.rs` :

```rust
//! L'application SkyShare (jalon 1) : la coquille Tauri — fenêtre, icône
//! près de l'horloge, instance unique, démarrage avec Windows.

pub mod demarrage;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;

/// Identifiant de l'icône près de l'horloge — la tâche 11 la retrouve par lui.
pub const ID_ICONE: &str = "principal";

pub fn lancer() {
    let au_demarrage = demarrage::lance_au_demarrage(std::env::args());
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![demarrage::ARGUMENT_DEMARRAGE]),
        ))
        .setup(move |app| {
            installer_icone(app.handle())?;
            #[cfg(not(debug_assertions))]
            activer_au_premier_lancement(app.handle());
            if !au_demarrage {
                montrer_fenetre(app.handle());
            }
            Ok(())
        })
        .on_window_event(|fenetre, evenement| {
            if let WindowEvent::CloseRequested { api, .. } = evenement {
                // Fermer réduit (spec D4) : l'application reste près de
                // l'horloge ; seul « Quitter » du menu de l'icône la ferme.
                api.prevent_close();
                let _ = fenetre.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("échec du lancement de SkyShare");
}

fn montrer_fenetre(app: &AppHandle) {
    if let Some(fenetre) = app.get_webview_window("main") {
        let _ = fenetre.unminimize();
        let _ = fenetre.show();
        let _ = fenetre.set_focus();
    }
}

fn installer_icone(app: &AppHandle) -> tauri::Result<()> {
    let ouvrir = MenuItem::with_id(app, "ouvrir", "Ouvrir SkyShare", true, None::<&str>)?;
    let quitter = MenuItem::with_id(app, "quitter", "Quitter", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&ouvrir, &quitter])?;
    TrayIconBuilder::with_id(ID_ICONE)
        .icon(tauri::include_image!("icons/32x32.png"))
        .tooltip("SkyShare")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, evenement| match evenement.id.as_ref() {
            "ouvrir" => montrer_fenetre(app),
            "quitter" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|icone, evenement| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = evenement {
                montrer_fenetre(icone.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Spec D4 : l'application démarre avec la session Windows. Activé UNE fois,
/// au premier lancement de la version publiée : si l'utilisateur le désactive
/// ensuite (Mon compte), il reste désactivé.
#[cfg(not(debug_assertions))]
fn activer_au_premier_lancement(app: &AppHandle) {
    use tauri_plugin_autostart::ManagerExt;
    if let Ok(dossier) = app.path().app_data_dir() {
        if demarrage::premier_lancement(&dossier).unwrap_or(false) {
            let _ = app.autolaunch().enable();
        }
    }
}
```

Au-dessus du module de tests de `demarrage.rs` :

```rust
//! Démarrage avec la session Windows (spec D4).

use std::path::Path;

/// Argument passé par l'extension de démarrage automatique : l'application
/// démarre alors cachée, près de l'horloge.
pub const ARGUMENT_DEMARRAGE: &str = "--demarrage";

pub fn lance_au_demarrage(arguments: impl IntoIterator<Item = String>) -> bool {
    arguments.into_iter().any(|a| a == ARGUMENT_DEMARRAGE)
}

/// `true` au tout premier appel pour ce dossier, `false` ensuite : un témoin
/// vide y est écrit.
pub fn premier_lancement(dossier: &Path) -> std::io::Result<bool> {
    let temoin = dossier.join("premier-lancement-fait");
    if temoin.exists() {
        return Ok(false);
    }
    std::fs::create_dir_all(dossier)?;
    std::fs::write(&temoin, b"")?;
    Ok(true)
}
```

```bash
cd spike && cargo test -p sky-app
```
Attendu : les deux tests de `demarrage` verts ; `une_seconde_instance_rend_la_main…` **FAIL** —
« la seconde instance tournait encore après 15 s ». C'est la preuve, écrite d'avance, que le test
voit l'absence de l'extension.

- [ ] **Étape 7 : brancher l'instance unique**

Dans `lancer()`, **avant** `.plugin(tauri_plugin_autostart::init(…))` (l'extension d'instance
unique doit être la première enregistrée) :

```rust
        // Spec D4 : une seule instance. La seconde remet la première au premier
        // plan puis s'arrête — elle ne synchronise jamais.
        .plugin(tauri_plugin_single_instance::init(|app, _arguments, _dossier| montrer_fenetre(app)))
```

```bash
cd spike && cargo test -p sky-app && cargo clippy --all-targets -- -D warnings
```
Attendu : trois tests verts.

- [ ] **Étape 8 : lancer l'application à la main**

Deux terminaux :
```bash
npm --prefix app run dev
```
```bash
cd spike && cargo run -p sky-app
```
Vérifier : la fenêtre s'ouvre sur la disposition (barre latérale, « SkyShare » en Instrument Serif,
fond `#16120F`) ; fermer la fenêtre ne termine pas le processus (`tasklist | findstr sky-app` le
montre encore) ; un second `cargo run -p sky-app` rend la main aussitôt et remet la fenêtre au
premier plan. L'icône près de l'horloge et son menu sont vérifiés par le propriétaire en T9 (ils
demandent une souris). Arrêter par `taskkill /IM sky-app.exe /F`.

- [ ] **Étape 9 : produire l'installateur**

```bash
npm --prefix app run build
cd spike/crates/sky-app && ../../../app/node_modules/.bin/tauri build
```
Attendu : `Finished 1 bundle at:` suivi de `…\spike\target\release\bundle\nsis\SkyShare_0.1.0_x64-setup.exe`.
Le premier `tauri build` télécharge l'outillage NSIS (réseau requis). Consigner la taille du
fichier produit dans le rapport. Ne pas lancer l'installateur sur la machine de développement dans
cette tâche (il activerait le démarrage automatique) : c'est l'objet de T9.

- [ ] **Étape 10 : commiter et pousser**

```bash
git add .gitignore
git add app/package.json
git add app/package-lock.json
git add app/index.html
git add app/vite.config.ts
git add app/tsconfig.json
git add app/src/main.tsx
git add app/src/styles.css
git add app/src/Disposition.tsx
git add app/src/App.tsx
git add app/src/App.test.tsx
git add app/src/test/installation.ts
git add spike/crates/sky-app/Cargo.toml
git add spike/crates/sky-app/build.rs
git add spike/crates/sky-app/tauri.conf.json
git add spike/crates/sky-app/capabilities/default.json
git add spike/crates/sky-app/icons/source.svg
git add spike/crates/sky-app/icons/32x32.png
git add spike/crates/sky-app/icons/128x128.png
git add spike/crates/sky-app/icons/128x128@2x.png
git add spike/crates/sky-app/icons/icon.ico
git add spike/crates/sky-app/icons/icon.png
git add spike/crates/sky-app/src/main.rs
git add spike/crates/sky-app/src/lib.rs
git add spike/crates/sky-app/src/demarrage.rs
git add spike/crates/sky-app/tests/instance_unique.rs
git add spike/Cargo.lock
git diff --cached --stat
git status --short
git commit -m "feat: squelette de l application — Tauri 2, icone, instance unique, installateur NSIS"
git push
```
`git status --short` ne doit montrer, hors index, que `AGENTS.md`, `testm4.md`, `.claude/` (non
suivis d'avant) — jamais `app/node_modules`, `app/dist`, `spike/crates/sky-app/gen`.

---

## Task 7 : Cœur `sky-app` — état et synchronisation

**Ce que la tâche livre :** le `Noyau`, seul détenteur de l'état ; la boucle de synchronisation
unique (30 s fenêtre visible, 5 min réduite, 2 s pendant un partage ou une attente), qui transmet
l'état précédent à chaque tour et **ne synchronise pas pendant une attente** (c'est le partage qui
le fait, sinon la boucle lui volerait les enveloppes) ; la connexion et la déconnexion, refusées
pendant un partage ; l'enregistrement automatique de l'appareil au premier lancement (nom de la
machine validé par `sky_compte::nom_appareil_valide`, repli « Appareil SkyShare », le même que
`sky-compte`) ; l'événement `etat`.

**Files:**
- Modify: `spike/crates/sky-app/Cargo.toml`, `spike/crates/sky-app/src/lib.rs`, `spike/Cargo.lock`
- Create: `spike/crates/sky-app/src/{cadence,reveil,vue,materiel,coquille,noyau,commandes,essais}.rs`
- Modify: `spike/crates/sky-compte/tests/faux_serveur/mod.rs` (champ `syncs_recues`)

**Interfaces:**
- Consumes : `sky_compte::{synchroniser, connecter, moi, rattacher_appareil, enregistrer_appareil, nom_appareil_valide, Coffre, Config, ErreurCompte, Etat, Jetons}` ; `sky_partage::rendez_vous::CADENCE`.
- Produces :
  - `vue.rs` : `Connexion { Deconnecte, EnCours, Connecte, SessionExpiree }` ; `AmiVue { id, friendship_id, nom, appareils: usize }` ; `DemandeVue { friendship_id, nom }` ; `ListeVue { id, nom, couleur, emoji, membres }` ; `AppareilVue { id, nom, courant, revoque }` ; `EcranVue { index: usize, nom: String, principal: bool }` ; `FinVue { Arrete, PasEnPartage { ami }, ReseauBloque, TropLente, SessionExpiree, AucuneDemande, Autre { message } }` ; `PartageVue { Inactif, Disponible { debut_ms: u64, fenetre_s: u64, ecran: usize }, Diffuse { spectateur: Option<String>, depuis_ms: u64, debit_mbps: f64, rtt_ms: f64, ecran: usize }, Demande { ami: String, debut_ms: u64 }, Regarde { ami: String, connecte_en_s: f64, debit_mbps: f64, images_par_s: u64, gigue_ms: f64, depuis_ms: u64 }, Termine { fin: FinVue } }` ; `Instantane { connexion, nom, code, amis, demandes, listes, appareils, partage, nvenc, ecrans, demarrage_automatique }` et `Instantane::vide(connexion: Connexion) -> Instantane`.
  - `cadence.rs` : `CADENCE_VISIBLE` (30 s), `CADENCE_REDUITE` (5 min), `CADENCE_PARTAGE` (= `sky_partage::rendez_vous::CADENCE`) ; `Phase { Inactive, Attente, EnCours }`, `Phase::de(&PartageVue) -> Phase` ; `cadence(visible: bool, phase: Phase) -> Duration`.
  - `reveil.rs` : `Reveil` (`sonner(&self)`, `attendre(&self, duree: Duration)`), `trait Sommeil { fn dormir(&mut self, duree: Duration, reveil: &Reveil); }`, `SommeilReel`.
  - `materiel.rs` : `NOM_D_APPAREIL_DE_REPLI: &str = "Appareil SkyShare"`, `nom_d_appareil(nom_machine: Option<&str>) -> String`.
  - `coquille.rs` : `trait Coquille: Send + Sync { fn publier_etat(&self, &Instantane); fn publier_partage(&self, &PartageVue); fn demarrage_automatique(&self, actif: bool) -> Result<(), String>; }` ; `CoquilleTauri::nouvelle(app: AppHandle)`.
  - `noyau.rs` : `Connecteur`, `Branchements { coquille, connecter, nom_machine }`, `MESSAGE_PENDANT_PARTAGE`, `MESSAGE_SESSION_EXPIREE`, `MESSAGE_NON_CONNECTE`, `message_erreur(&ErreurCompte) -> String`, `permis_hors_partage(Phase) -> Result<(), String>` ; `Noyau::{nouveau, config, coffre, reveil, instantane, phase, definir_visible, definir_ecrans, definir_demarrage_automatique_connu, synchroniser, demarrer, connexion, deconnexion, tour, boucle}` ; `pub(crate)` : `publier`, `apres_commande`, `apres_erreur`, `exiger_connexion`, `modifier_partage`, `forcer_partage` (tests).
  - `commandes.rs` : `etat_courant`, `connexion`, `deconnexion` (Tauri), `sur_un_fil`.
  - `essais.rs` (tests seulement) : `CoquilleEspion`, `Contexte { serveur, noyau, coquille, connexions }`, `contexte(prefixe: &str, connecte: bool) -> Contexte`.
  - Serveur double : `EtatFaux::syncs_recues: Vec<Option<u64>>`.

- [ ] **Étape 1 : dépendances et double**

`spike/crates/sky-app/Cargo.toml`, `[dependencies]` gagne :

```toml
sky-compte = { version = "0.0.0", path = "../sky-compte" }
sky-partage = { version = "0.0.0", path = "../sky-partage" }
```
et, nouvelle section :
```toml
# Le serveur double de `sky-compte` (inclus par `#[path]` dans lib.rs, tests
# seulement) dépend de ces deux-là.
[dev-dependencies]
tiny_http = "0.12"
base64.workspace = true
```

Dans `spike/crates/sky-compte/tests/faux_serveur/mod.rs`, à la fin de `EtatFaux` :

```rust
    /// `?version=` reçu à chaque `GET /api/sky/sync` autorisé, dans l'ordre
    /// (jalon 1, tâche 7) — prouve qu'un client transmet l'état précédent.
    pub syncs_recues: Vec<Option<u64>>,
```
et dans `gerer_sync`, juste après le `if let Some(refus) = autoriser_appel(…) { return refus; }` :
```rust
    e.syncs_recues.push(version_connue);
```

- [ ] **Étape 2 : écrire les tests purs qui échouent**

Créer chaque fichier avec son seul module de tests.

`spike/crates/sky-app/src/cadence.rs` :

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::vue::FinVue;

    #[test]
    fn la_cadence_suit_la_fenetre_et_le_partage() {
        // Spec §3. Neutralisation : ignorer `visible` — 5 min rendues 30 s.
        assert_eq!(cadence(true, Phase::Inactive), Duration::from_secs(30));
        assert_eq!(cadence(false, Phase::Inactive), Duration::from_secs(300));
        assert_eq!(cadence(false, Phase::Attente), Duration::from_secs(2));
        assert_eq!(cadence(true, Phase::EnCours), Duration::from_secs(2));
    }

    #[test]
    fn la_phase_se_lit_dans_le_partage() {
        assert_eq!(Phase::de(&PartageVue::Inactif), Phase::Inactive);
        assert_eq!(Phase::de(&PartageVue::Disponible { debut_ms: 0, fenetre_s: 1800, ecran: 0 }), Phase::Attente);
        assert_eq!(Phase::de(&PartageVue::Demande { ami: "bob".into(), debut_ms: 0 }), Phase::Attente);
        assert_eq!(
            Phase::de(&PartageVue::Diffuse { spectateur: None, depuis_ms: 0, debit_mbps: 0.0, rtt_ms: 0.0, ecran: 0 }),
            Phase::EnCours
        );
        assert_eq!(Phase::de(&PartageVue::Termine { fin: FinVue::Arrete }), Phase::Inactive);
    }
}
```

`spike/crates/sky-app/src/reveil.rs` :

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Instant;

    #[test]
    fn un_reveil_interrompt_le_sommeil() {
        // Neutralisation : retirer `notify_all` de `sonner` — le sommeil dure 10 s.
        let reveil = Arc::new(Reveil::default());
        let depuis_un_fil = Arc::clone(&reveil);
        let fil = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            depuis_un_fil.sonner();
        });
        let debut = Instant::now();
        reveil.attendre(Duration::from_secs(10));
        fil.join().unwrap();
        assert!(debut.elapsed() < Duration::from_secs(2));
    }
}
```

`spike/crates/sky-app/src/materiel.rs` :

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_nom_de_machine_que_le_site_refuse_devient_pc() {
        // Spec §10 : validé par la règle de sky-compte (1 à 64 unités UTF-16,
        // sans NUL). Neutralisation : retirer le `filter` — le nom de 65 unités
        // passe, le site le refuserait à l'enregistrement.
        assert_eq!(nom_d_appareil(Some("BUREAU-KILLIAN")), "BUREAU-KILLIAN");
        assert_eq!(nom_d_appareil(Some(&"x".repeat(65))), NOM_D_APPAREIL_DE_REPLI);
        assert_eq!(nom_d_appareil(Some("a\u{0}b")), NOM_D_APPAREIL_DE_REPLI);
        assert_eq!(nom_d_appareil(Some("")), NOM_D_APPAREIL_DE_REPLI);
        assert_eq!(nom_d_appareil(None), NOM_D_APPAREIL_DE_REPLI);
    }
}
```

`spike/crates/sky-app/src/vue.rs` :

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn le_partage_est_serialise_sous_les_noms_que_lit_l_interface() {
        // Contrat avec app/src/types.ts. Neutralisation : retirer
        // `rename_all_fields` — `connecteEnS` devient `connecte_en_s`.
        let v = serde_json::to_value(PartageVue::Regarde {
            ami: "bob".into(),
            connecte_en_s: 0.5,
            debit_mbps: 12.0,
            images_par_s: 60,
            gigue_ms: 5.0,
            depuis_ms: 1,
        })
        .unwrap();
        assert_eq!(
            v,
            json!({"etat": "regarde", "ami": "bob", "connecteEnS": 0.5, "debitMbps": 12.0,
                   "imagesParS": 60, "gigueMs": 5.0, "depuisMs": 1})
        );
        let fin = serde_json::to_value(PartageVue::Termine { fin: FinVue::PasEnPartage { ami: "bob".into() } }).unwrap();
        assert_eq!(fin, json!({"etat": "termine", "fin": {"cause": "pas_en_partage", "ami": "bob"}}));
    }

    #[test]
    fn l_instantane_est_en_camel_case_et_la_connexion_en_snake_case() {
        let v = serde_json::to_value(Instantane::vide(Connexion::SessionExpiree)).unwrap();
        assert_eq!(v["connexion"], "session_expiree");
        assert_eq!(v["demarrageAutomatique"], false);
        assert_eq!(v["partage"], json!({"etat": "inactif"}));
    }
}
```

Déclarer dans `lib.rs` : `pub mod cadence; pub mod materiel; pub mod reveil; pub mod vue;`.

```bash
cd spike && cargo test -p sky-app
```
Attendu : ÉCHEC à la compilation (`cadence`, `Reveil`, `nom_d_appareil`, `PartageVue`… introuvables).

- [ ] **Étape 3 : implémenter les modules purs**

`vue.rs`, au-dessus du module de tests :

```rust
//! Les instantanés envoyés à l'interface : le SEUL contrat entre le cœur et
//! `app/src/types.ts`. Aucun jeton, aucune clé, aucune adresse (spec §3,
//! frontière D5) : l'interface affiche, elle ne détient rien.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Connexion {
    Deconnecte,
    EnCours,
    Connecte,
    SessionExpiree,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AmiVue {
    pub id: i64,
    pub friendship_id: i64,
    pub nom: String,
    pub appareils: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemandeVue {
    pub friendship_id: i64,
    pub nom: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListeVue {
    pub id: i64,
    pub nom: String,
    pub couleur: Option<String>,
    pub emoji: Option<String>,
    pub membres: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppareilVue {
    pub id: i64,
    pub nom: String,
    /// Cet appareil-ci : jamais révocable depuis l'application.
    pub courant: bool,
    pub revoque: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EcranVue {
    /// Rang dans l'énumération de Windows — celui qu'attend `WgcCapture::new`.
    pub index: usize,
    pub nom: String,
    pub principal: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "cause", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum FinVue {
    Arrete,
    PasEnPartage { ami: String },
    ReseauBloque,
    TropLente,
    SessionExpiree,
    AucuneDemande,
    Autre { message: String },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "etat", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum PartageVue {
    Inactif,
    /// Hôte disponible : une demande sera honorée jusqu'à `debut + fenetre`.
    Disponible { debut_ms: u64, fenetre_s: u64, ecran: usize },
    /// Hôte : un ami regarde (ou se connecte).
    Diffuse { spectateur: Option<String>, depuis_ms: u64, debit_mbps: f64, rtt_ms: f64, ecran: usize },
    /// Spectateur : demande envoyée, réponse attendue.
    Demande { ami: String, debut_ms: u64 },
    /// Spectateur : connecté, flux mesuré puis jeté (spec D2).
    Regarde { ami: String, connecte_en_s: f64, debit_mbps: f64, images_par_s: u64, gigue_ms: f64, depuis_ms: u64 },
    Termine { fin: FinVue },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Instantane {
    pub connexion: Connexion,
    pub nom: Option<String>,
    pub code: Option<String>,
    pub amis: Vec<AmiVue>,
    pub demandes: Vec<DemandeVue>,
    pub listes: Vec<ListeVue>,
    pub appareils: Vec<AppareilVue>,
    pub partage: PartageVue,
    /// Carte NVIDIA utilisable : sans elle, « Partager » est désactivé.
    pub nvenc: bool,
    pub ecrans: Vec<EcranVue>,
    pub demarrage_automatique: bool,
}

impl Instantane {
    pub fn vide(connexion: Connexion) -> Instantane {
        Instantane {
            connexion,
            nom: None,
            code: None,
            amis: Vec::new(),
            demandes: Vec::new(),
            listes: Vec::new(),
            appareils: Vec::new(),
            partage: PartageVue::Inactif,
            nvenc: false,
            ecrans: Vec::new(),
            demarrage_automatique: false,
        }
    }
}
```

`cadence.rs` :

```rust
//! Les cadences de la boucle de synchronisation (spec §3). Points de départ
//! de la spec, pas des mesures.

use std::time::Duration;

use crate::vue::PartageVue;

pub const CADENCE_VISIBLE: Duration = Duration::from_secs(30);
pub const CADENCE_REDUITE: Duration = Duration::from_secs(5 * 60);
/// La même que la négociation (C2) : une seule source.
pub const CADENCE_PARTAGE: Duration = sky_partage::rendez_vous::CADENCE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Inactive,
    /// Hôte disponible ou spectateur qui attend la réponse : c'est le partage
    /// qui synchronise, jamais la boucle.
    Attente,
    /// Flux établi.
    EnCours,
}

impl Phase {
    pub fn de(partage: &PartageVue) -> Phase {
        match partage {
            PartageVue::Disponible { .. } | PartageVue::Demande { .. } => Phase::Attente,
            PartageVue::Diffuse { .. } | PartageVue::Regarde { .. } => Phase::EnCours,
            PartageVue::Inactif | PartageVue::Termine { .. } => Phase::Inactive,
        }
    }
}

pub fn cadence(visible: bool, phase: Phase) -> Duration {
    match phase {
        Phase::Attente | Phase::EnCours => CADENCE_PARTAGE,
        Phase::Inactive if visible => CADENCE_VISIBLE,
        Phase::Inactive => CADENCE_REDUITE,
    }
}
```

`reveil.rs` :

```rust
//! Le sommeil de la boucle, interruptible : un changement de fenêtre ou la
//! fin d'un partage change la cadence tout de suite, pas au bout de 5 min.

use std::sync::{Condvar, Mutex};
use std::time::Duration;

#[derive(Default)]
pub struct Reveil {
    sonne: Mutex<bool>,
    condition: Condvar,
}

impl Reveil {
    pub fn sonner(&self) {
        *self.sonne.lock().expect("réveil empoisonné") = true;
        self.condition.notify_all();
    }

    /// Dort au plus `duree`, moins si `sonner` est appelé. Un réveil sonné
    /// pendant le tour précédent fait rendre la main aussitôt : c'est voulu.
    pub fn attendre(&self, duree: Duration) {
        let garde = self.sonne.lock().expect("réveil empoisonné");
        let (mut garde, _) = self
            .condition
            .wait_timeout_while(garde, duree, |sonne| !*sonne)
            .expect("réveil empoisonné");
        *garde = false;
    }
}

/// Le temps tel que la boucle le voit — injecté pour que les tests ne dorment pas.
pub trait Sommeil {
    fn dormir(&mut self, duree: Duration, reveil: &Reveil);
}

pub struct SommeilReel;

impl Sommeil for SommeilReel {
    fn dormir(&mut self, duree: Duration, reveil: &Reveil) {
        reveil.attendre(duree);
    }
}
```

`materiel.rs` :

```rust
//! Ce que l'application apprend de la machine.

/// Nom d'appareil quand celui de la machine est refusé par le site (spec §10).
/// MÊME valeur que le repli de `sky_compte` (`nom_par_defaut`, annuaire.rs) :
/// un seul produit, un seul nom générique — arbitrage du contrôleur. Changer
/// l'un sans l'autre ferait apparaître deux noms pour le même cas.
pub const NOM_D_APPAREIL_DE_REPLI: &str = "Appareil SkyShare";

/// Le nom de la machine s'il passe la règle que `sky-compte` applique déjà
/// (celle de `POST /api/sky/devices`), sinon « Appareil SkyShare ».
pub fn nom_d_appareil(nom_machine: Option<&str>) -> String {
    nom_machine
        .filter(|nom| sky_compte::nom_appareil_valide(nom))
        .map(str::to_string)
        .unwrap_or_else(|| NOM_D_APPAREIL_DE_REPLI.to_string())
}
```

```bash
cd spike && cargo test -p sky-app
```
Attendu : les tests purs verts (ceux de `demarrage` et `instance_unique` aussi).

- [ ] **Étape 4 : écrire la coquille, l'outillage de test et les tests du noyau qui échouent**

`spike/crates/sky-app/src/coquille.rs` :

```rust
//! La frontière avec Tauri : le noyau publie par ce trait, sans rien savoir
//! de la fenêtre. Les tests y branchent un espion.

use tauri::{AppHandle, Emitter};
use tauri_plugin_autostart::ManagerExt;

use crate::vue::{Instantane, PartageVue};

pub trait Coquille: Send + Sync {
    /// Événement `etat` : l'instantané complet, après chaque changement.
    fn publier_etat(&self, instantane: &Instantane);
    /// Événement `partage` : le partage en cours, à chaque événement de `sky-partage`.
    fn publier_partage(&self, partage: &PartageVue);
    fn demarrage_automatique(&self, actif: bool) -> Result<(), String>;
}

pub struct CoquilleTauri {
    app: AppHandle,
}

impl CoquilleTauri {
    pub fn nouvelle(app: AppHandle) -> CoquilleTauri {
        CoquilleTauri { app }
    }
}

impl Coquille for CoquilleTauri {
    fn publier_etat(&self, instantane: &Instantane) {
        let _ = self.app.emit("etat", instantane);
    }

    fn publier_partage(&self, partage: &PartageVue) {
        let _ = self.app.emit("partage", partage);
    }

    fn demarrage_automatique(&self, actif: bool) -> Result<(), String> {
        let gestionnaire = self.app.autolaunch();
        let issue = if actif { gestionnaire.enable() } else { gestionnaire.disable() };
        issue.map_err(|e| e.to_string())
    }
}
```

`spike/crates/sky-app/src/essais.rs` :

```rust
//! Outillage des tests du cœur — jamais compilé hors tests.

use std::sync::{Arc, Mutex};

use sky_compte::{Coffre, Config, Jetons};

use crate::coquille::Coquille;
use crate::faux_serveur::FauxServeur;
use crate::noyau::{Branchements, Noyau};
use crate::vue::{Instantane, PartageVue};

#[derive(Default)]
pub(crate) struct CoquilleEspion {
    pub etats: Mutex<Vec<Instantane>>,
    pub partages: Mutex<Vec<PartageVue>>,
    pub demarrages: Mutex<Vec<bool>>,
}

impl Coquille for Arc<CoquilleEspion> {
    fn publier_etat(&self, instantane: &Instantane) {
        self.etats.lock().unwrap().push(instantane.clone());
    }
    fn publier_partage(&self, partage: &PartageVue) {
        self.partages.lock().unwrap().push(partage.clone());
    }
    fn demarrage_automatique(&self, actif: bool) -> Result<(), String> {
        self.demarrages.lock().unwrap().push(actif);
        Ok(())
    }
}

pub(crate) struct Contexte {
    pub serveur: FauxServeur,
    pub noyau: Arc<Noyau>,
    pub coquille: Arc<CoquilleEspion>,
    /// Nombre d'appels au connecteur (qui, en production, ouvre le navigateur).
    pub connexions: Arc<Mutex<u32>>,
}

/// Un noyau branché sur un serveur double. `connecte` : des jetons valides
/// sont déjà dans le coffre. La connexion simulée range un jeton reconnu par
/// le double, sans navigateur.
pub(crate) fn contexte(prefixe: &str, connecte: bool) -> Contexte {
    let serveur = FauxServeur::demarrer();
    let jeton = serveur.jeton_de_test();
    let coffre = Coffre::pour_test(prefixe);
    if connecte {
        coffre.ranger_jetons(&Jetons { session: jeton.clone(), renouvellement: "peu-importe".into() }).unwrap();
    }
    let coquille = Arc::new(CoquilleEspion::default());
    let connexions = Arc::new(Mutex::new(0u32));
    let compteur = Arc::clone(&connexions);
    let noyau = Arc::new(Noyau::nouveau(
        Config::vers(&serveur.url()),
        coffre,
        Branchements {
            coquille: Box::new(Arc::clone(&coquille)),
            connecter: Box::new(move |_config, coffre| {
                *compteur.lock().unwrap() += 1;
                let jetons = Jetons { session: jeton.clone(), renouvellement: "peu-importe".into() };
                coffre.ranger_jetons(&jetons)?;
                Ok(jetons)
            }),
            nom_machine: Some("MACHINE-DE-TEST".into()),
        },
    ));
    Contexte { serveur, noyau, coquille, connexions }
}
```

Dans `lib.rs`, ajouter (`commandes` viendra à l'étape 6) :

```rust
pub mod coquille;
pub mod noyau;

#[cfg(test)]
mod essais;
// Le serveur double de `sky-compte`, partagé plutôt que recopié : un second
// double divergerait du premier, qui est dérivé du code du site.
#[cfg(test)]
#[allow(dead_code)]
#[path = "../../sky-compte/tests/faux_serveur/mod.rs"]
mod faux_serveur;
```

Créer `spike/crates/sky-app/src/noyau.rs` avec seulement son module de tests :

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::cadence::{CADENCE_PARTAGE, CADENCE_REDUITE, CADENCE_VISIBLE};
    use crate::essais::contexte;
    use std::sync::Arc;

    struct SommeilEspion {
        noyau: Arc<Noyau>,
        durees: Vec<Duration>,
    }

    impl Sommeil for SommeilEspion {
        fn dormir(&mut self, duree: Duration, _reveil: &Reveil) {
            self.durees.push(duree);
            if self.durees.len() == 1 {
                self.noyau.definir_visible(false);
            }
        }
    }

    #[test]
    fn la_boucle_transmet_l_etat_precedent_et_passe_a_5_minutes_fenetre_reduite() {
        // Neutralisations : (1) passer toujours `None` à `synchroniser` —
        // `syncs_recues` vaut [None, None, None] ; (2) ignorer `visible` dans
        // `tour` — la seconde durée reste 30 s.
        let c = contexte("sky-test-app-boucle", true);
        c.serveur.etat_mut().version = 4;
        let mut sommeil = SommeilEspion { noyau: Arc::clone(&c.noyau), durees: Vec::new() };
        c.noyau.boucle(&mut sommeil, Some(3));
        assert_eq!(c.serveur.etat_mut().syncs_recues, vec![None, Some(4), Some(4)]);
        assert_eq!(sommeil.durees, vec![CADENCE_VISIBLE, CADENCE_REDUITE]);
    }

    #[test]
    fn pendant_une_attente_la_boucle_ne_synchronise_pas_et_bat_toutes_les_2_s() {
        // Sinon la boucle consommerait l'offre d'un ami, que le serveur efface
        // en la livrant. Neutralisation : retirer `phase != Phase::Attente` de
        // `tour` — une synchronisation apparaît.
        let c = contexte("sky-test-app-attente", true);
        c.noyau.forcer_partage(PartageVue::Disponible { debut_ms: 0, fenetre_s: 1800, ecran: 0 });
        let mut sommeil = SommeilEspion { noyau: Arc::clone(&c.noyau), durees: Vec::new() };
        c.noyau.boucle(&mut sommeil, Some(2));
        assert!(c.serveur.etat_mut().syncs_recues.is_empty());
        assert_eq!(sommeil.durees, vec![CADENCE_PARTAGE]);
    }

    #[test]
    fn au_premier_lancement_l_appareil_s_enregistre_sous_le_nom_de_la_machine() {
        // Spec §4. Neutralisation : rendre `Ok(())` dans la branche `None` de
        // `assurer_appareil` — aucun appareil n'est enregistré.
        let c = contexte("sky-test-app-premier-lancement", true);
        c.noyau.demarrer();
        let appareils = c.serveur.etat_mut().appareils.clone();
        assert_eq!(appareils.len(), 1);
        assert_eq!(appareils[0].nom, "MACHINE-DE-TEST");
        let vue = c.noyau.instantane();
        assert!(vue.appareils[0].courant, "l'appareil enregistré est celui de cette machine");
        assert_eq!(vue.connexion, Connexion::Connecte);
    }

    #[test]
    fn la_connexion_enregistre_l_appareil_et_synchronise() {
        let c = contexte("sky-test-app-connexion", false);
        assert_eq!(c.noyau.instantane().connexion, Connexion::Deconnecte);
        c.noyau.connexion().unwrap();
        assert_eq!(*c.connexions.lock().unwrap(), 1);
        assert_eq!(c.noyau.instantane().connexion, Connexion::Connecte);
        assert_eq!(c.serveur.etat_mut().appareils.len(), 1);
        assert!(c.noyau.instantane().code.is_some(), "l'état a été synchronisé");
    }

    #[test]
    fn pendant_un_partage_ni_connexion_ni_deconnexion() {
        // Spec §6 : un login révoque puis réenregistre l'appareil ; pendant une
        // attente, les enveloppes adressées à l'ancien seraient perdues.
        // Neutralisation : retirer `permis_hors_partage` de `connexion` — le
        // connecteur est appelé (compteur à 1).
        let c = contexte("sky-test-app-pas-pendant-partage", true);
        c.noyau.forcer_partage(PartageVue::Demande { ami: "bob".into(), debut_ms: 0 });
        assert_eq!(c.noyau.connexion(), Err(MESSAGE_PENDANT_PARTAGE.to_string()));
        assert_eq!(*c.connexions.lock().unwrap(), 0);
        assert_eq!(c.noyau.deconnexion(), Err(MESSAGE_PENDANT_PARTAGE.to_string()));
        assert!(c.noyau.coffre().jetons().unwrap().is_some(), "la session est intacte");
    }

    #[test]
    fn une_session_refusee_passe_en_session_expiree_et_publie() {
        // Neutralisation : ne pas changer `connexion` sur `Refuse` — reste Connecte.
        let c = contexte("sky-test-app-session-expiree", true);
        c.serveur.etat_mut().refuser_tout = true;
        assert!(matches!(c.noyau.synchroniser(), Err(ErreurCompte::Refuse)));
        assert_eq!(c.noyau.instantane().connexion, Connexion::SessionExpiree);
        let publies = c.coquille.etats.lock().unwrap();
        assert_eq!(publies.last().map(|i| i.connexion), Some(Connexion::SessionExpiree));
    }
}
```

```bash
cd spike && cargo test -p sky-app
```
Attendu : ÉCHEC à la compilation (`Noyau` introuvable).

- [ ] **Étape 5 : implémenter le noyau**

Au-dessus du module de tests de `noyau.rs` :

```rust
//! Le cœur de l'application : le SEUL endroit qui tient l'état de la machine
//! (spec §3). Aucune dépendance à Tauri : la fenêtre, l'icône et les
//! événements passent par `Coquille`, ce qui rend chaque commande testable
//! sans fenêtre, contre le serveur double.
//!
//! ORDRE DES VERROUS : `synchro` puis `donnees`, jamais l'inverse. `donnees`
//! n'est JAMAIS tenu pendant un appel réseau, un accès au trousseau ni un
//! appel à la coquille.
//!
//! RESYNCHRONISATION COMPLÈTE APRÈS CHAQUE COMMANDE : le site calcule la
//! version comme le MAX des `updated_at` des lignes qui RESTENT (`etat.ts`).
//! Retirer un ami ou supprimer une liste supprime une ligne ; régénérer le
//! code écrit une colonne hors du calcul. La version peut ne pas bouger, et
//! `?version=` rendrait `inchange` en gardant l'ancien état à l'écran.

use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use sky_compte::{Coffre, Config, ErreurCompte, Etat, Jetons};

use crate::cadence::{cadence, Phase};
use crate::coquille::Coquille;
use crate::materiel::nom_d_appareil;
use crate::reveil::{Reveil, Sommeil};
use crate::vue::{AmiVue, AppareilVue, Connexion, DemandeVue, EcranVue, Instantane, ListeVue, PartageVue};

pub type Connecteur = Box<dyn Fn(&Config, &Coffre) -> Result<Jetons, ErreurCompte> + Send + Sync>;

pub struct Branchements {
    pub coquille: Box<dyn Coquille>,
    /// `sky_compte::connecter` en production (ouvre le navigateur, attend au
    /// plus 5 minutes) ; une fermeture sans navigateur dans les tests.
    pub connecter: Connecteur,
    /// `COMPUTERNAME` en production.
    pub nom_machine: Option<String>,
}

pub const MESSAGE_PENDANT_PARTAGE: &str = "Impossible pendant un partage ou une attente : arrête-le d'abord.";
pub const MESSAGE_SESSION_EXPIREE: &str = "Session expirée — reconnecte-toi";
pub const MESSAGE_NON_CONNECTE: &str = "Connecte-toi d'abord.";

/// Le message montré pour une erreur de compte. Jamais de jeton : les
/// variantes d'`ErreurCompte` n'en portent pas (voir `erreur.rs`).
pub fn message_erreur(e: &ErreurCompte) -> String {
    match e {
        ErreurCompte::Refuse => MESSAGE_SESSION_EXPIREE.to_string(),
        ErreurCompte::Reseau(_) => "Le site SkyShare ne répond pas — vérifie ta connexion internet.".to_string(),
        // Le `Display` d'`ErreurCompte::Protocole` préfixe « réponse inattendue
        // du serveur », faux pour une entrée refusée AVANT le réseau (listes,
        // nom d'appareil) : seul le détail est montré.
        ErreurCompte::Protocole(detail) => detail.clone(),
        ErreurCompte::Coffre(detail) => format!("Gestionnaire d'identifiants de Windows : {detail}"),
    }
}

/// Spec §6 : un `login` révoque puis réenregistre l'appareil (C2). Pendant un
/// partage ou une attente, les enveloppes adressées à l'ancien seraient
/// perdues : l'application ne se (dé)connecte jamais à ce moment-là.
pub fn permis_hors_partage(phase: Phase) -> Result<(), String> {
    if phase == Phase::Inactive {
        Ok(())
    } else {
        Err(MESSAGE_PENDANT_PARTAGE.to_string())
    }
}

struct Donnees {
    connexion: Connexion,
    nom: Option<String>,
    /// Dernier état reçu, enveloppes vidées : le précédent du tour suivant.
    etat: Option<Etat>,
    appareil_courant: Option<i64>,
    /// Le prochain tour synchronisera SANS précédent (voir l'en-tête).
    resynchro_complete: bool,
    visible: bool,
    partage: PartageVue,
    nvenc: bool,
    ecrans: Vec<EcranVue>,
    demarrage_automatique: bool,
}

pub struct Noyau {
    config: Config,
    coffre: Coffre,
    branchements: Branchements,
    donnees: Mutex<Donnees>,
    /// Sérialise les synchronisations : une seule à la fois, qu'elle vienne
    /// de la boucle, d'une commande ou d'un partage.
    synchro: Mutex<()>,
    reveil: Reveil,
}

impl Noyau {
    pub fn nouveau(config: Config, coffre: Coffre, branchements: Branchements) -> Noyau {
        let connecte = matches!(coffre.jetons(), Ok(Some(_)));
        Noyau {
            config,
            coffre,
            branchements,
            donnees: Mutex::new(Donnees {
                connexion: if connecte { Connexion::Connecte } else { Connexion::Deconnecte },
                nom: None,
                etat: None,
                appareil_courant: None,
                resynchro_complete: true,
                visible: true,
                partage: PartageVue::Inactif,
                nvenc: false,
                ecrans: Vec::new(),
                demarrage_automatique: false,
            }),
            synchro: Mutex::new(()),
            reveil: Reveil::default(),
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn coffre(&self) -> &Coffre {
        &self.coffre
    }

    pub fn reveil(&self) -> &Reveil {
        &self.reveil
    }

    fn donnees(&self) -> MutexGuard<'_, Donnees> {
        self.donnees.lock().expect("verrou des données empoisonné")
    }

    pub fn instantane(&self) -> Instantane {
        let d = self.donnees();
        let etat = d.etat.as_ref();
        Instantane {
            connexion: d.connexion,
            nom: d.nom.clone(),
            code: etat.map(|e| e.code.clone()),
            amis: etat
                .map(|e| {
                    e.amis
                        .iter()
                        .map(|a| AmiVue {
                            id: a.id,
                            friendship_id: a.friendship_id,
                            nom: a.discord_name.clone(),
                            appareils: a.appareils.len(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            demandes: etat
                .map(|e| {
                    e.demandes
                        .iter()
                        .map(|dm| DemandeVue { friendship_id: dm.friendship_id, nom: dm.discord_name.clone() })
                        .collect()
                })
                .unwrap_or_default(),
            listes: etat
                .map(|e| {
                    e.listes
                        .iter()
                        .map(|l| ListeVue {
                            id: l.id,
                            nom: l.nom.clone(),
                            couleur: l.couleur.clone(),
                            emoji: l.emoji.clone(),
                            membres: l.membres.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            appareils: etat
                .map(|e| {
                    e.appareils
                        .iter()
                        .map(|a| AppareilVue {
                            id: a.id,
                            nom: a.nom.clone(),
                            courant: d.appareil_courant == Some(a.id),
                            revoque: a.revoked_at.is_some(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            partage: d.partage.clone(),
            nvenc: d.nvenc,
            ecrans: d.ecrans.clone(),
            demarrage_automatique: d.demarrage_automatique,
        }
    }

    pub(crate) fn publier(&self) {
        let instantane = self.instantane();
        self.branchements.coquille.publier_etat(&instantane);
    }

    pub fn phase(&self) -> Phase {
        Phase::de(&self.donnees().partage)
    }

    pub fn definir_visible(&self, visible: bool) {
        self.donnees().visible = visible;
        self.reveil.sonner();
    }

    pub fn definir_ecrans(&self, ecrans: Vec<EcranVue>) {
        self.donnees().ecrans = ecrans;
        self.publier();
    }

    pub fn definir_demarrage_automatique_connu(&self, actif: bool) {
        self.donnees().demarrage_automatique = actif;
        self.publier();
    }

    /// Change le partage affiché, publie `partage` puis `etat`, et réveille la
    /// boucle (la cadence dépend de la phase).
    pub(crate) fn modifier_partage(&self, partage: PartageVue) {
        self.donnees().partage = partage.clone();
        self.branchements.coquille.publier_partage(&partage);
        self.publier();
        self.reveil.sonner();
    }

    #[cfg(test)]
    pub(crate) fn forcer_partage(&self, partage: PartageVue) {
        self.donnees().partage = partage;
    }

    /// La SEULE synchronisation de l'application : la boucle, les commandes
    /// et le partage passent tous par elle. Transmet l'état précédent (ou
    /// aucun, après une commande), garde le nouvel état enveloppes vidées,
    /// publie, et rend l'état complet — enveloppes comprises — à l'appelant :
    /// pendant une attente, c'est le partage.
    pub fn synchroniser(&self) -> Result<Etat, ErreurCompte> {
        let _une_a_la_fois = self.synchro.lock().expect("verrou de synchronisation empoisonné");
        let precedent = {
            let d = self.donnees();
            if d.resynchro_complete {
                None
            } else {
                d.etat.clone()
            }
        };
        let issue = sky_compte::synchroniser(&self.config, &self.coffre, precedent.as_ref());
        let appareil_courant = self.coffre.identifiant_appareil().ok().flatten();
        {
            let mut d = self.donnees();
            match &issue {
                Ok(etat) => {
                    d.etat = Some(Etat { enveloppes: Vec::new(), ..etat.clone() });
                    d.resynchro_complete = false;
                    d.connexion = Connexion::Connecte;
                    d.appareil_courant = appareil_courant;
                }
                Err(ErreurCompte::Refuse) if d.connexion == Connexion::Connecte => {
                    d.connexion = Connexion::SessionExpiree;
                }
                Err(_) => {}
            }
        }
        self.publier();
        issue
    }

    /// Après une commande réussie : resynchronisation complète (voir
    /// l'en-tête). Pendant une attente, c'est le partage qui fera le tour :
    /// la commande ne synchronise pas elle-même, elle le lui demande.
    pub(crate) fn apres_commande(&self) {
        let phase = {
            let mut d = self.donnees();
            d.resynchro_complete = true;
            Phase::de(&d.partage)
        };
        if phase == Phase::Attente {
            self.publier();
        } else {
            let _ = self.synchroniser();
        }
    }

    /// Traduit une erreur de compte en message, et passe en « session
    /// expirée » sur un refus.
    pub(crate) fn apres_erreur(&self, e: &ErreurCompte) -> String {
        if matches!(e, ErreurCompte::Refuse) {
            self.donnees().connexion = Connexion::SessionExpiree;
            self.publier();
        }
        message_erreur(e)
    }

    pub(crate) fn exiger_connexion(&self) -> Result<(), String> {
        if self.donnees().connexion == Connexion::Connecte {
            Ok(())
        } else {
            Err(MESSAGE_NON_CONNECTE.to_string())
        }
    }

    /// Au lancement : si une session existe, s'assure qu'un appareil est
    /// enregistré (spec §4), lit le nom Discord et synchronise.
    pub fn demarrer(&self) {
        if self.donnees().connexion != Connexion::Connecte {
            self.publier();
            return;
        }
        let nom = sky_compte::moi(&self.config, &self.coffre).ok().map(|m| m.discord_name);
        self.donnees().nom = nom;
        if let Err(e) = self.assurer_appareil(false) {
            self.apres_erreur(&e);
        }
        let _ = self.synchroniser();
    }

    /// Aucun appareil dans le coffre : l'enregistrer sous le nom de la
    /// machine. Un appareil, juste après une connexion : le rattacher à la
    /// nouvelle session (C2, revue finale I1 — sans quoi plus aucune
    /// enveloppe ne lui serait livrée).
    fn assurer_appareil(&self, apres_connexion: bool) -> Result<(), ErreurCompte> {
        match self.coffre.identifiant_appareil()? {
            Some(_) if apres_connexion => sky_compte::rattacher_appareil(&self.config, &self.coffre).map(|_| ()),
            Some(_) => Ok(()),
            None => {
                let nom = nom_d_appareil(self.branchements.nom_machine.as_deref());
                let cle = self.coffre.identite()?.public_key();
                sky_compte::enregistrer_appareil(&self.config, &self.coffre, &nom, &cle).map(|_| ())
            }
        }
    }

    pub fn connexion(&self) -> Result<(), String> {
        permis_hors_partage(self.phase())?;
        self.donnees().connexion = Connexion::EnCours;
        self.publier();
        if let Err(e) = (self.branchements.connecter)(&self.config, &self.coffre) {
            self.donnees().connexion = Connexion::Deconnecte;
            self.publier();
            return Err(message_erreur(&e));
        }
        let nom = sky_compte::moi(&self.config, &self.coffre).ok().map(|m| m.discord_name);
        {
            let mut d = self.donnees();
            d.nom = nom;
            d.connexion = Connexion::Connecte;
            d.resynchro_complete = true;
        }
        let appareil = self.assurer_appareil(true);
        let synchronisation = self.synchroniser();
        appareil.map_err(|e| {
            format!(
                "Connecté, mais cet appareil n'a pas pu être rattaché ({}) : il ne recevra aucune \
                 demande de partage. Reconnecte-toi.",
                message_erreur(&e)
            )
        })?;
        synchronisation.map(|_| ()).map_err(|e| message_erreur(&e))
    }

    pub fn deconnexion(&self) -> Result<(), String> {
        permis_hors_partage(self.phase())?;
        self.coffre.oublier().map_err(|e| message_erreur(&e))?;
        {
            let mut d = self.donnees();
            d.connexion = Connexion::Deconnecte;
            d.nom = None;
            d.etat = None;
            d.resynchro_complete = true;
        }
        self.publier();
        Ok(())
    }

    /// Un tour de boucle : synchronise (sauf pendant une attente), rend la
    /// durée du prochain sommeil.
    pub fn tour(&self) -> Duration {
        let (connecte, phase) = {
            let d = self.donnees();
            (d.connexion == Connexion::Connecte, Phase::de(&d.partage))
        };
        // Pendant une attente, c'est le partage qui synchronise : la boucle lui
        // volerait les enveloppes, que le serveur efface en les livrant.
        if connecte && phase != Phase::Attente {
            let _ = self.synchroniser();
        }
        let d = self.donnees();
        cadence(d.visible, Phase::de(&d.partage))
    }

    /// La boucle de synchronisation unique (spec §3). `tours` : `None` en
    /// production, un nombre dans les tests.
    pub fn boucle(&self, sommeil: &mut dyn Sommeil, tours: Option<usize>) {
        let mut n = 0usize;
        loop {
            let duree = self.tour();
            n += 1;
            if tours.is_some_and(|t| n >= t) {
                return;
            }
            sommeil.dormir(duree, &self.reveil);
        }
    }
}
```

`spike/crates/sky-app/src/commandes.rs` :

```rust
//! Les commandes Tauri : des intentions de l'interface, confiées au `Noyau`
//! sur un fil dédié (`spawn_blocking`) — jamais sur le fil de l'interface :
//! `sky-compte` bloque le temps d'une requête (5 s au plus), `connexion` le
//! temps d'une connexion Discord (5 minutes au plus). C'est le seul fichier du
//! cœur qui contient des `async fn`.

use std::sync::Arc;

use tauri::State;

use crate::noyau::Noyau;
use crate::vue::Instantane;

pub(crate) async fn sur_un_fil<T: Send + 'static>(
    noyau: &Arc<Noyau>,
    travail: impl FnOnce(&Arc<Noyau>) -> T + Send + 'static,
) -> Result<T, String> {
    let noyau = Arc::clone(noyau);
    tauri::async_runtime::spawn_blocking(move || travail(&noyau)).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn etat_courant(noyau: State<'_, Arc<Noyau>>) -> Result<Instantane, String> {
    Ok(noyau.instantane())
}

#[tauri::command]
pub async fn connexion(noyau: State<'_, Arc<Noyau>>) -> Result<(), String> {
    sur_un_fil(noyau.inner(), |n| n.connexion()).await?
}

#[tauri::command]
pub async fn deconnexion(noyau: State<'_, Arc<Noyau>>) -> Result<(), String> {
    sur_un_fil(noyau.inner(), |n| n.deconnexion()).await?
}
```

- [ ] **Étape 6 : brancher le noyau dans `lancer()`**

Dans `lib.rs`, ajouter les `use` :

```rust
use std::sync::Arc;

use sky_compte::{Coffre, Config};
use tauri_plugin_autostart::ManagerExt;

use crate::coquille::CoquilleTauri;
use crate::noyau::{Branchements, Noyau};
use crate::reveil::SommeilReel;
```

(retirer alors le `use tauri_plugin_autostart::ManagerExt;` local d'`activer_au_premier_lancement`).
Dans `lancer()`, après les deux `.plugin(…)`, ajouter :

```rust
        .invoke_handler(tauri::generate_handler![
            commandes::etat_courant,
            commandes::connexion,
            commandes::deconnexion,
        ])
```

Remplacer le corps de `.setup(move |app| { … })` par :

```rust
            installer_icone(app.handle())?;
            #[cfg(not(debug_assertions))]
            activer_au_premier_lancement(app.handle());
            let noyau = Arc::new(Noyau::nouveau(
                Config::depuis_env(),
                Coffre::nouveau()?,
                Branchements {
                    coquille: Box::new(CoquilleTauri::nouvelle(app.handle().clone())),
                    connecter: Box::new(sky_compte::connecter),
                    nom_machine: std::env::var("COMPUTERNAME").ok(),
                },
            ));
            noyau.definir_demarrage_automatique_connu(app.autolaunch().is_enabled().unwrap_or(false));
            noyau.definir_visible(!au_demarrage);
            app.manage(Arc::clone(&noyau));
            // La boucle unique, sur son propre fil (spec §3).
            std::thread::Builder::new().name("synchronisation".into()).spawn(move || {
                noyau.demarrer();
                noyau.boucle(&mut SommeilReel, None);
            })?;
            if !au_demarrage {
                montrer_fenetre(app.handle());
            }
            Ok(())
```

Remplacer le corps de `.on_window_event(|fenetre, evenement| { … })` par :

```rust
            let noyau = fenetre.try_state::<Arc<Noyau>>();
            match evenement {
                WindowEvent::CloseRequested { api, .. } => {
                    // Fermer réduit (spec D4) : seul « Quitter » ferme.
                    api.prevent_close();
                    let _ = fenetre.hide();
                    if let Some(noyau) = noyau {
                        noyau.definir_visible(false);
                    }
                }
                WindowEvent::Focused(true) => {
                    if let Some(noyau) = noyau {
                        noyau.definir_visible(true);
                    }
                }
                WindowEvent::Resized(_) if fenetre.is_minimized().unwrap_or(false) => {
                    if let Some(noyau) = noyau {
                        noyau.definir_visible(false);
                    }
                }
                _ => {}
            }
```

Et dans `montrer_fenetre`, à la fin :

```rust
    if let Some(noyau) = app.try_state::<Arc<Noyau>>() {
        noyau.definir_visible(true);
    }
```

Déclarer enfin dans `lib.rs` : `pub mod commandes;`.

- [ ] **Étape 7 : lancer**

```bash
cd spike && cargo test -p sky-app && cargo test -p sky-compte && cargo clippy --all-targets -- -D warnings
```
Attendu : tous verts, dont les six tests du noyau, les deux de `vue`, celui de `reveil`, celui de
`materiel`, les deux de `cadence`. `sky-compte` inchangé (le champ ajouté au double ne casse rien).

- [ ] **Étape 8 : prouver par neutralisation (une à la fois)** chaque test ajouté, selon son
commentaire. Pour la boucle, faire les deux neutralisations **séparément**.

- [ ] **Étape 9 : commiter et pousser**

```bash
git add spike/crates/sky-app/Cargo.toml
git add spike/crates/sky-app/src/lib.rs
git add spike/crates/sky-app/src/cadence.rs
git add spike/crates/sky-app/src/reveil.rs
git add spike/crates/sky-app/src/vue.rs
git add spike/crates/sky-app/src/materiel.rs
git add spike/crates/sky-app/src/coquille.rs
git add spike/crates/sky-app/src/noyau.rs
git add spike/crates/sky-app/src/commandes.rs
git add spike/crates/sky-app/src/essais.rs
git add spike/crates/sky-compte/tests/faux_serveur/mod.rs
git add spike/Cargo.lock
git diff --cached --stat
git commit -m "feat: coeur de l application — etat, boucle de synchronisation unique, connexion"
git push
```

---

## Task 8 : Interface — Connexion et Amis

**Ce que la tâche livre :** l'écran Connexion (un bouton, le navigateur s'ouvre, l'application se
met à jour seule au retour ; « Session expirée — reconnecte-toi ») et l'écran Amis (code ami +
Ajouter, demandes reçues + Accepter, amis avec leur nombre d'appareils, **Regarder** actif seulement
si l'ami a au moins un appareil et qu'aucun partage n'est en cours, menu « … » avec Retirer et
Bloquer) ; les commandes `ajouter_ami`, `accepter_ami`, `retirer_ami`, `bloquer_ami`.

Le bouton Regarder appelle la commande `regarder`, qui n'existe qu'à la tâche 11 : entre-temps, un
clic rend l'erreur de Tauri « commande introuvable », affichée telle quelle. C'est assumé (le
premier essai réel, T9, ne l'utilise pas).

**Files:**
- Modify: `spike/crates/sky-app/src/noyau.rs`, `spike/crates/sky-app/src/commandes.rs`, `spike/crates/sky-app/src/lib.rs` (`generate_handler!`)
- Create: `app/src/types.ts`, `app/src/pont.ts`, `app/src/useInstantane.ts`, `app/src/test/fabriques.ts`, `app/src/ecrans/Connexion.tsx`, `app/src/ecrans/Connexion.test.tsx`, `app/src/ecrans/Amis.tsx`, `app/src/ecrans/Amis.test.tsx`
- Modify: `app/src/App.tsx`, `app/src/App.test.tsx`

**Interfaces:**
- Consumes : `Noyau::{exiger_connexion, apres_commande, apres_erreur, donnees}` (T7) ; `sky_compte::{normaliser_code_ami, ajouter_ami, accepter_ami, retirer_ami, bloquer_ami, AjoutAmi, Acceptation, Retrait, Blocage}`.
- Produces (Rust) : `MESSAGE_CODE_MAL_FORME: &str` ; `Noyau::ajouter_ami(&self, saisie: &str) -> Result<String, String>`, `accepter_ami(&self, friendship_id: i64) -> Result<String, String>`, `retirer_ami(&self, friendship_id: i64) -> Result<String, String>`, `bloquer_ami(&self, friendship_id: i64) -> Result<String, String>` ; commandes Tauri `ajouter_ami(code: String)`, `accepter_ami(friendship_id: i64)`, `retirer_ami(friendship_id: i64)`, `bloquer_ami(friendship_id: i64)`, toutes `-> Result<String, String>`.
- Produces (interface) : les types de `types.ts` (miroir exact de `vue.rs`) ; `pont.{etatCourant, ecouterEtat, connexion, deconnexion, ajouterAmi, accepterAmi, retirerAmi, bloquerAmi, regarder}` ; `useInstantane(): Instantane | null` ; `instantaneDeTest(partiel?: Partial<Instantane>): Instantane` ; composants `Connexion({ connexion })`, `Amis({ instantane })`.

- [ ] **Étape 1 : écrire les tests du noyau qui échouent**

Dans le module `tests` de `noyau.rs`, ajouter :

```rust
    #[test]
    fn on_ne_s_ajoute_pas_soi_meme_et_rien_ne_part() {
        // Le site refuserait par un 400 sans message utile. Neutralisation :
        // retirer la comparaison au code de l'état — la demande part
        // (`appels_amis` à 1).
        let c = contexte("sky-test-app-propre-code", true);
        c.noyau.synchroniser().unwrap(); // code « FAUX2345 » du double
        assert_eq!(c.noyau.ajouter_ami("sky-faux-2345"), Err("C'est ton propre code ami.".to_string()));
        assert_eq!(c.serveur.etat_mut().appels_amis, 0);
    }

    #[test]
    fn une_saisie_mal_formee_ne_part_pas() {
        let c = contexte("sky-test-app-code-mal-forme", true);
        assert_eq!(c.noyau.ajouter_ami("pas un code"), Err(MESSAGE_CODE_MAL_FORME.to_string()));
        assert_eq!(c.serveur.etat_mut().appels_amis, 0);
    }

    #[test]
    fn retirer_un_ami_le_fait_disparaitre_meme_si_la_version_ne_bouge_pas() {
        // Le double, comme le site, ne fait pas progresser la version sur un
        // retrait. Neutralisation : retirer `d.resynchro_complete = true` de
        // `apres_commande` — `inchange`, l'ami reste affiché.
        let c = contexte("sky-test-app-retirer", true);
        let mut ami = crate::faux_serveur::AmiFaux::sans_appareil("bob");
        ami.friendship_id = 777;
        c.serveur.etat_mut().amis.push(ami);
        c.noyau.synchroniser().unwrap();
        assert_eq!(c.noyau.instantane().amis.len(), 1);

        assert_eq!(c.noyau.retirer_ami(777), Ok("Ami retiré.".to_string()));
        assert!(c.noyau.instantane().amis.is_empty());
    }
```

```bash
cd spike && cargo test -p sky-app
```
Attendu : ÉCHEC à la compilation (`ajouter_ami`, `MESSAGE_CODE_MAL_FORME`, `retirer_ami` introuvables).

- [ ] **Étape 2 : implémenter les commandes d'amis**

Dans `noyau.rs`, ajouter aux `use` : `use sky_compte::{Acceptation, AjoutAmi, Blocage, Retrait};`
et, après `MESSAGE_NON_CONNECTE` :

```rust
pub const MESSAGE_CODE_MAL_FORME: &str =
    "Ce code ami n'a pas la bonne forme : 8 caractères, par exemple SKY-ABCD-EFGH.";
const MESSAGE_AMI_DISPARU: &str = "Cet ami n'est plus dans ta liste.";
```

Dans `impl Noyau`, après `deconnexion` :

```rust
    pub fn ajouter_ami(&self, saisie: &str) -> Result<String, String> {
        self.exiger_connexion()?;
        let code = sky_compte::normaliser_code_ami(saisie).ok_or_else(|| MESSAGE_CODE_MAL_FORME.to_string())?;
        // Le site refuse de s'ajouter soi-même par un 400 muet : le dire ici.
        if self.donnees().etat.as_ref().is_some_and(|e| e.code == code) {
            return Err("C'est ton propre code ami.".to_string());
        }
        let issue = sky_compte::ajouter_ami(&self.config, &self.coffre, &code).map_err(|e| self.apres_erreur(&e))?;
        self.apres_commande();
        match issue {
            AjoutAmi::Envoyee { .. } => Ok("Demande envoyée.".to_string()),
            // Couvre aussi « ce compte t'a bloqué » : le site ne les distingue
            // pas, l'application non plus.
            AjoutAmi::CodeIntrouvable => Err("Code ami introuvable.".to_string()),
            AjoutAmi::DejaDemandee => Err("Une demande existe déjà avec ce compte.".to_string()),
        }
    }

    pub fn accepter_ami(&self, friendship_id: i64) -> Result<String, String> {
        self.exiger_connexion()?;
        let issue = sky_compte::accepter_ami(&self.config, &self.coffre, friendship_id)
            .map_err(|e| self.apres_erreur(&e))?;
        self.apres_commande();
        match issue {
            Acceptation::Acceptee => Ok("Demande acceptée.".to_string()),
            Acceptation::Introuvable => Err("Cette demande n'existe plus.".to_string()),
        }
    }

    pub fn retirer_ami(&self, friendship_id: i64) -> Result<String, String> {
        self.exiger_connexion()?;
        let issue = sky_compte::retirer_ami(&self.config, &self.coffre, friendship_id)
            .map_err(|e| self.apres_erreur(&e))?;
        self.apres_commande();
        match issue {
            Retrait::Retire => Ok("Ami retiré.".to_string()),
            Retrait::Introuvable => Err(MESSAGE_AMI_DISPARU.to_string()),
        }
    }

    pub fn bloquer_ami(&self, friendship_id: i64) -> Result<String, String> {
        self.exiger_connexion()?;
        let issue = sky_compte::bloquer_ami(&self.config, &self.coffre, friendship_id)
            .map_err(|e| self.apres_erreur(&e))?;
        self.apres_commande();
        match issue {
            Blocage::Bloque => Ok("Ami bloqué.".to_string()),
            Blocage::Introuvable => Err(MESSAGE_AMI_DISPARU.to_string()),
        }
    }
```

Dans `commandes.rs` :

```rust
#[tauri::command]
pub async fn ajouter_ami(noyau: State<'_, Arc<Noyau>>, code: String) -> Result<String, String> {
    sur_un_fil(noyau.inner(), move |n| n.ajouter_ami(&code)).await?
}

#[tauri::command]
pub async fn accepter_ami(noyau: State<'_, Arc<Noyau>>, friendship_id: i64) -> Result<String, String> {
    sur_un_fil(noyau.inner(), move |n| n.accepter_ami(friendship_id)).await?
}

#[tauri::command]
pub async fn retirer_ami(noyau: State<'_, Arc<Noyau>>, friendship_id: i64) -> Result<String, String> {
    sur_un_fil(noyau.inner(), move |n| n.retirer_ami(friendship_id)).await?
}

#[tauri::command]
pub async fn bloquer_ami(noyau: State<'_, Arc<Noyau>>, friendship_id: i64) -> Result<String, String> {
    sur_un_fil(noyau.inner(), move |n| n.bloquer_ami(friendship_id)).await?
}
```

Dans `lib.rs`, `generate_handler!` gagne `commandes::ajouter_ami, commandes::accepter_ami,
commandes::retirer_ami, commandes::bloquer_ami,`.

```bash
cd spike && cargo test -p sky-app && cargo clippy --all-targets -- -D warnings
```
Attendu : PASS. Neutraliser les deux tests à neutralisation écrite ; rétablir.

- [ ] **Étape 3 : l'interface — types, pont, instantané**

`app/src/types.ts` — miroir de `spike/crates/sky-app/src/vue.rs` (le test
`le_partage_est_serialise_sous_les_noms_que_lit_l_interface` fige les noms côté Rust) :

```ts
// Miroir EXACT de spike/crates/sky-app/src/vue.rs. Un nom changé là-bas doit
// l'être ici : le test Rust `le_partage_est_serialise_sous_les_noms_que_lit_l_interface`
// fige la forme émise.

export type Connexion = "deconnecte" | "en_cours" | "connecte" | "session_expiree";

export interface AmiVue {
  id: number;
  friendshipId: number;
  nom: string;
  appareils: number;
}

export interface DemandeVue {
  friendshipId: number;
  nom: string;
}

export interface ListeVue {
  id: number;
  nom: string;
  couleur: string | null;
  emoji: string | null;
  /** Identifiants d'UTILISATEUR (AmiVue.id), jamais d'amitié. */
  membres: number[];
}

export interface AppareilVue {
  id: number;
  nom: string;
  courant: boolean;
  revoque: boolean;
}

export interface EcranVue {
  index: number;
  nom: string;
  principal: boolean;
}

export type FinVue =
  | { cause: "arrete" }
  | { cause: "pas_en_partage"; ami: string }
  | { cause: "reseau_bloque" }
  | { cause: "trop_lente" }
  | { cause: "session_expiree" }
  | { cause: "aucune_demande" }
  | { cause: "autre"; message: string };

export type PartageVue =
  | { etat: "inactif" }
  | { etat: "disponible"; debutMs: number; fenetreS: number; ecran: number }
  | { etat: "diffuse"; spectateur: string | null; depuisMs: number; debitMbps: number; rttMs: number; ecran: number }
  | { etat: "demande"; ami: string; debutMs: number }
  | {
      etat: "regarde";
      ami: string;
      connecteEnS: number;
      debitMbps: number;
      imagesParS: number;
      gigueMs: number;
      depuisMs: number;
    }
  | { etat: "termine"; fin: FinVue };

export interface Instantane {
  connexion: Connexion;
  nom: string | null;
  code: string | null;
  amis: AmiVue[];
  demandes: DemandeVue[];
  listes: ListeVue[];
  appareils: AppareilVue[];
  partage: PartageVue;
  nvenc: boolean;
  ecrans: EcranVue[];
  demarrageAutomatique: boolean;
}
```

`app/src/pont.ts` :

```ts
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Instantane } from "./types";

/**
 * Le SEUL module qui parle au cœur Rust. L'interface n'a ni jeton, ni clé,
 * ni accès réseau (spec §3, frontière D5) : elle lit des instantanés et envoie
 * des intentions. Les noms d'arguments sont en camelCase : Tauri 2 les
 * traduit vers les paramètres snake_case des commandes (`friendshipId` →
 * `friendship_id`).
 */
export const pont = {
  etatCourant: () => invoke<Instantane>("etat_courant"),
  ecouterEtat: (rappel: (instantane: Instantane) => void): Promise<UnlistenFn> =>
    listen<Instantane>("etat", (evenement) => rappel(evenement.payload)),
  connexion: () => invoke<void>("connexion"),
  deconnexion: () => invoke<void>("deconnexion"),
  ajouterAmi: (code: string) => invoke<string>("ajouter_ami", { code }),
  accepterAmi: (friendshipId: number) => invoke<string>("accepter_ami", { friendshipId }),
  retirerAmi: (friendshipId: number) => invoke<string>("retirer_ami", { friendshipId }),
  bloquerAmi: (friendshipId: number) => invoke<string>("bloquer_ami", { friendshipId }),
  /** `ami` : identifiant d'UTILISATEUR. Commande ajoutée à la tâche 11. */
  regarder: (ami: number) => invoke<void>("regarder", { ami }),
};
```

`app/src/useInstantane.ts` :

```ts
import { useEffect, useState } from "react";
import { pont } from "./pont";
import type { Instantane } from "./types";

/**
 * Le dernier instantané du cœur : lu une fois au montage (un événement émis
 * avant que l'interface écoute serait perdu), puis tenu à jour par `etat`.
 */
export function useInstantane(): Instantane | null {
  const [instantane, setInstantane] = useState<Instantane | null>(null);
  useEffect(() => {
    let actif = true;
    let arreter: (() => void) | undefined;
    void pont.etatCourant().then((etat) => {
      if (actif) setInstantane(etat);
    });
    void pont
      .ecouterEtat((etat) => {
        if (actif) setInstantane(etat);
      })
      .then((desabonner) => {
        if (actif) arreter = desabonner;
        else desabonner();
      });
    return () => {
      actif = false;
      arreter?.();
    };
  }, []);
  return instantane;
}
```

`app/src/test/fabriques.ts` :

```ts
import type { Instantane } from "../types";

/** Un instantané plausible, connecté, sans ami ni partage. */
export function instantaneDeTest(partiel: Partial<Instantane> = {}): Instantane {
  return {
    connexion: "connecte",
    nom: "Killian",
    code: "ABCD2345",
    amis: [],
    demandes: [],
    listes: [],
    appareils: [],
    partage: { etat: "inactif" },
    nvenc: true,
    ecrans: [{ index: 0, nom: "Écran 1", principal: true }],
    demarrageAutomatique: true,
    ...partiel,
  };
}
```

- [ ] **Étape 4 : écrire les tests d'écrans qui échouent**

`app/src/ecrans/Connexion.test.tsx` :

```tsx
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { pont } from "../pont";
import { Connexion } from "./Connexion";

vi.mock("../pont", () => ({ pont: { connexion: vi.fn() } }));

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(pont.connexion).mockResolvedValue(undefined);
});

describe("Connexion", () => {
  it("dit en clair que la session a expiré", () => {
    // Spec §4, quatrième échec. Neutralisation : retirer le paragraphe
    // `session_expiree` — le test rougit.
    render(<Connexion connexion="session_expiree" />);
    expect(screen.getByRole("alert")).toHaveTextContent("Session expirée — reconnecte-toi");
  });

  it("le bouton lance la connexion, et se désactive pendant qu'elle court", async () => {
    const { rerender } = render(<Connexion connexion="deconnecte" />);
    await userEvent.click(screen.getByRole("button", { name: "Se connecter avec Discord" }));
    expect(pont.connexion).toHaveBeenCalledTimes(1);
    rerender(<Connexion connexion="en_cours" />);
    expect(screen.getByRole("button", { name: "Se connecter avec Discord" })).toBeDisabled();
  });
});
```

`app/src/ecrans/Amis.test.tsx` :

```tsx
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { pont } from "../pont";
import { instantaneDeTest } from "../test/fabriques";
import { Amis } from "./Amis";

vi.mock("../pont", () => ({
  pont: { ajouterAmi: vi.fn(), accepterAmi: vi.fn(), retirerAmi: vi.fn(), bloquerAmi: vi.fn(), regarder: vi.fn() },
}));

beforeEach(() => vi.clearAllMocks());

const ALICE = { id: 1, friendshipId: 11, nom: "Alice", appareils: 0 };
const BOB = { id: 2, friendshipId: 12, nom: "Bob", appareils: 1 };

function ligne(nom: string) {
  return within(screen.getByText(nom).closest("li")!);
}

describe("Amis", () => {
  it("Regarder est inactif pour un ami sans appareil, actif avec un appareil", () => {
    // Spec §4 et §8. Neutralisation : retirer `sansAppareil ||` du `disabled`.
    render(<Amis instantane={instantaneDeTest({ amis: [ALICE, BOB] })} />);
    expect(ligne("Alice").getByRole("button", { name: "Regarder" })).toBeDisabled();
    expect(ligne("Bob").getByRole("button", { name: "Regarder" })).toBeEnabled();
  });

  it("Regarder est inactif pendant un partage", () => {
    // Neutralisation : retirer `|| occupe` du `disabled`.
    render(
      <Amis
        instantane={instantaneDeTest({ amis: [BOB], partage: { etat: "disponible", debutMs: 0, fenetreS: 1800, ecran: 0 } })}
      />,
    );
    expect(ligne("Bob").getByRole("button", { name: "Regarder" })).toBeDisabled();
  });

  it("accepter et retirer envoient l'identifiant d'AMITIÉ, regarder celui de l'utilisateur", async () => {
    // Les routes friends/{id} lisent un friendshipId (T3). Neutralisation :
    // passer `ami.id` à `retirerAmi` — appelé avec 2 au lieu de 12.
    vi.mocked(pont.accepterAmi).mockResolvedValue("Demande acceptée.");
    vi.mocked(pont.retirerAmi).mockResolvedValue("Ami retiré.");
    vi.mocked(pont.regarder).mockResolvedValue(undefined);
    render(
      <Amis instantane={instantaneDeTest({ amis: [BOB], demandes: [{ friendshipId: 55, nom: "Carole" }] })} />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Accepter" }));
    expect(pont.accepterAmi).toHaveBeenCalledWith(55);
    await userEvent.click(ligne("Bob").getByRole("button", { name: "Regarder" }));
    expect(pont.regarder).toHaveBeenCalledWith(2);
    await userEvent.click(screen.getByRole("button", { name: "Plus d'actions pour Bob" }));
    await userEvent.click(screen.getByRole("button", { name: "Retirer" }));
    expect(pont.retirerAmi).toHaveBeenCalledWith(12);
    expect(await screen.findByRole("status")).toHaveTextContent("Ami retiré.");
  });

  it("un refus du cœur s'affiche tel quel", async () => {
    vi.mocked(pont.ajouterAmi).mockRejectedValue("Code ami introuvable.");
    render(<Amis instantane={instantaneDeTest()} />);
    await userEvent.type(screen.getByLabelText("Code ami"), "SKY-ABCD-EFGH");
    await userEvent.click(screen.getByRole("button", { name: "Ajouter" }));
    expect(pont.ajouterAmi).toHaveBeenCalledWith("SKY-ABCD-EFGH");
    expect(await screen.findByRole("status")).toHaveTextContent("Code ami introuvable.");
  });
});
```

Remplacer `app/src/App.test.tsx` par :

```tsx
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import { pont } from "./pont";
import { instantaneDeTest } from "./test/fabriques";

vi.mock("./pont", () => ({ pont: { etatCourant: vi.fn(), ecouterEtat: vi.fn(), connexion: vi.fn() } }));

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(pont.etatCourant).mockResolvedValue(instantaneDeTest());
  vi.mocked(pont.ecouterEtat).mockResolvedValue(() => {});
});

describe("disposition", () => {
  it("la barre latérale mène aux trois écrans et garde le partage en bas", async () => {
    // Spec D6. Neutralisation : retirer l'entrée « Mon compte » de Disposition.
    render(<App />);
    const navigation = await screen.findByRole("navigation", { name: "Navigation" });
    for (const nom of ["Amis", "Listes", "Mon compte"]) {
      expect(within(navigation).getByRole("button", { name: nom })).toBeInTheDocument();
    }
    expect(within(navigation).getByRole("button", { name: "Partager mon écran" })).toBeInTheDocument();
    await userEvent.click(within(navigation).getByRole("button", { name: "Listes" }));
    expect(screen.getByRole("heading", { name: "Listes" })).toBeInTheDocument();
  });

  it("sans session, seul l'écran de connexion s'affiche", async () => {
    // Neutralisation : afficher la disposition quel que soit `connexion`.
    vi.mocked(pont.etatCourant).mockResolvedValue(instantaneDeTest({ connexion: "deconnecte" }));
    render(<App />);
    expect(await screen.findByRole("button", { name: "Se connecter avec Discord" })).toBeInTheDocument();
    expect(screen.queryByRole("navigation", { name: "Navigation" })).toBeNull();
  });
});
```

```bash
npm --prefix app test
```
Attendu : FAIL — `./ecrans/Connexion`, `./ecrans/Amis` introuvables ; `App` ne lit pas encore
l'instantané.

- [ ] **Étape 5 : écrire les écrans**

`app/src/ecrans/Connexion.tsx` :

```tsx
import { useState } from "react";
import { pont } from "../pont";
import type { Connexion as EtatConnexion } from "../types";

/** Premier lancement ou session expirée (spec §4). */
export function Connexion({ connexion }: { connexion: EtatConnexion }) {
  const [erreur, setErreur] = useState<string | null>(null);
  const enCours = connexion === "en_cours";
  return (
    <main className="flex h-screen flex-col items-center justify-center gap-6 bg-fond font-corps text-texte">
      <h1 className="font-titre text-5xl">SkyShare</h1>
      {connexion === "session_expiree" && (
        <p role="alert" className="text-alerte">
          Session expirée — reconnecte-toi
        </p>
      )}
      <button
        type="button"
        disabled={enCours}
        className="rounded-md bg-accent px-5 py-3 text-fond disabled:opacity-50"
        onClick={() => {
          setErreur(null);
          pont.connexion().catch((e: unknown) => setErreur(String(e)));
        }}
      >
        Se connecter avec Discord
      </button>
      {enCours && <p className="text-texte-2">Termine la connexion dans ton navigateur, puis reviens ici.</p>}
      {erreur && (
        <p role="alert" className="text-alerte">
          {erreur}
        </p>
      )}
    </main>
  );
}
```

`app/src/ecrans/Amis.tsx` :

```tsx
import { useState } from "react";
import { pont } from "../pont";
import type { AmiVue, Instantane } from "../types";

type Executer = (action: () => Promise<string | void>) => Promise<void>;

/** Un partage ou une attente en cours : on ne regarde personne en même temps. */
function partageEnCours(instantane: Instantane): boolean {
  return instantane.partage.etat !== "inactif" && instantane.partage.etat !== "termine";
}

export function Amis({ instantane }: { instantane: Instantane }) {
  const [code, setCode] = useState("");
  const [message, setMessage] = useState<string | null>(null);
  const occupe = partageEnCours(instantane);

  const executer: Executer = async (action) => {
    try {
      const retour = await action();
      setMessage(typeof retour === "string" ? retour : null);
    } catch (erreur) {
      setMessage(String(erreur));
    }
  };

  return (
    <section className="flex max-w-2xl flex-col gap-6">
      <h1 className="font-titre text-4xl">Amis</h1>
      <form
        className="flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          void executer(() => pont.ajouterAmi(code));
        }}
      >
        <label htmlFor="code-ami" className="sr-only">
          Code ami
        </label>
        <input
          id="code-ami"
          value={code}
          onChange={(e) => setCode(e.target.value)}
          placeholder="SKY-ABCD-EFGH"
          className="flex-1 rounded-md border border-bordure bg-surface px-3 py-2"
        />
        <button type="submit" className="rounded-md bg-accent px-4 py-2 text-fond">
          Ajouter
        </button>
      </form>
      {message && (
        <p role="status" className="text-texte-2">
          {message}
        </p>
      )}
      {instantane.demandes.length > 0 && (
        <section aria-label="Demandes reçues" className="flex flex-col gap-2">
          <h2 className="text-texte-2">Demandes reçues</h2>
          <ul className="flex flex-col gap-2">
            {instantane.demandes.map((demande) => (
              <li key={demande.friendshipId} className="flex items-center justify-between rounded-md bg-surface px-3 py-2">
                <span>{demande.nom}</span>
                <button type="button" onClick={() => void executer(() => pont.accepterAmi(demande.friendshipId))}>
                  Accepter
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
      <ul aria-label="Mes amis" className="flex flex-col gap-2">
        {instantane.amis.map((ami) => (
          <LigneAmi key={ami.friendshipId} ami={ami} occupe={occupe} executer={executer} />
        ))}
      </ul>
      {instantane.amis.length === 0 && (
        <p className="text-texte-3">Aucun ami pour l'instant : ton code ami est dans Mon compte.</p>
      )}
    </section>
  );
}

function LigneAmi({ ami, occupe, executer }: { ami: AmiVue; occupe: boolean; executer: Executer }) {
  const [menu, setMenu] = useState(false);
  const sansAppareil = ami.appareils === 0;
  return (
    <li className="flex items-center gap-3 rounded-md bg-surface px-3 py-2">
      <span className="flex-1">{ami.nom}</span>
      <span className="text-texte-3">
        {ami.appareils} appareil{ami.appareils > 1 ? "s" : ""}
      </span>
      <button
        type="button"
        disabled={sansAppareil || occupe}
        title={sansAppareil ? `${ami.nom} n'a aucun appareil enregistré` : undefined}
        className="rounded-md bg-surface-haute px-3 py-1 disabled:opacity-40"
        onClick={() => void executer(() => pont.regarder(ami.id))}
      >
        Regarder
      </button>
      <button type="button" aria-label={`Plus d'actions pour ${ami.nom}`} aria-expanded={menu} onClick={() => setMenu(!menu)}>
        …
      </button>
      {menu && (
        <span className="flex gap-2">
          <button type="button" onClick={() => void executer(() => pont.retirerAmi(ami.friendshipId))}>
            Retirer
          </button>
          <button type="button" onClick={() => void executer(() => pont.bloquerAmi(ami.friendshipId))}>
            Bloquer
          </button>
        </span>
      )}
    </li>
  );
}
```

Remplacer `app/src/App.tsx` par :

```tsx
import { useState } from "react";
import { Disposition, type Ecran } from "./Disposition";
import { Amis } from "./ecrans/Amis";
import { Connexion } from "./ecrans/Connexion";
import { useInstantane } from "./useInstantane";

const TITRES: Record<Ecran, string> = { amis: "Amis", listes: "Listes", compte: "Mon compte" };

export function App() {
  const instantane = useInstantane();
  const [ecran, setEcran] = useState<Ecran>("amis");
  if (instantane === null) return <p className="p-8 text-texte-2">Chargement…</p>;
  if (instantane.connexion !== "connecte") return <Connexion connexion={instantane.connexion} />;
  return (
    <Disposition
      ecran={ecran}
      choisir={setEcran}
      bas={
        <button type="button" disabled className="w-full rounded-md bg-accent px-3 py-2 text-fond disabled:opacity-50">
          Partager mon écran
        </button>
      }
    >
      {ecran === "amis" ? <Amis instantane={instantane} /> : <h1 className="font-titre text-4xl">{TITRES[ecran]}</h1>}
    </Disposition>
  );
}
```

- [ ] **Étape 6 : lancer, neutraliser, construire**

```bash
npm --prefix app test && npm --prefix app run build
```
Attendu : 8 tests verts. Appliquer chaque neutralisation écrite en tête de test, une à la fois ;
rétablir.

- [ ] **Étape 7 : vérifier le pont à la main**

`npm --prefix app run dev` puis `cd spike && cargo run -p sky-app` : l'écran de connexion
s'affiche si le trousseau de la machine n'a pas de session ; sinon l'écran Amis montre les amis
réels du compte **de production** — ne cliquer ni Retirer ni Bloquer ici (ce sont de vraies
personnes). Ouvrir les outils de développement de la fenêtre (clic droit → Inspecter, disponible en
mode développement) et vérifier dans la console qu'aucune erreur d'invocation n'apparaît au
chargement.

- [ ] **Étape 8 : commiter et pousser**

```bash
git add spike/crates/sky-app/src/noyau.rs
git add spike/crates/sky-app/src/commandes.rs
git add spike/crates/sky-app/src/lib.rs
git add app/src/types.ts
git add app/src/pont.ts
git add app/src/useInstantane.ts
git add app/src/test/fabriques.ts
git add app/src/ecrans/Connexion.tsx
git add app/src/ecrans/Connexion.test.tsx
git add app/src/ecrans/Amis.tsx
git add app/src/ecrans/Amis.test.tsx
git add app/src/App.tsx
git add app/src/App.test.tsx
git diff --cached --stat
git commit -m "feat: ecrans Connexion et Amis, commandes d amis"
git push
```

---

## Task 9 : Point d'arrêt — premier essai réel (propriétaire)

**Pourquoi maintenant (spec §8, leçon du 19/09/2026).** Le pont entre deux systèmes est exactement ce
qu'aucun test unitaire ne couvre. La connexion depuis l'application, l'enregistrement automatique de
l'appareil et l'ajout d'un ami sont le plus petit chemin de bout en bout : il s'éprouve **avant**
d'écrire les écrans suivants. **L'implémenteur ne peut pas faire cette tâche** : elle demande deux
machines, deux comptes Discord et le propriétaire.

**Files:**
- Modify: `tasks/todo.md` (section « Jalon 1 — essai réel 1 », résultats consignés par le contrôleur)

- [ ] **Étape 1 (contrôleur) : mettre en production la tâche 1, sur accord**

Depuis la tâche 2, `sky-compte` **exige** `listes[].membres` : sans la tâche 1 en production, toute
synchronisation échoue en erreur de protocole. Demander au propriétaire, **en le disant ainsi** :
« La tâche 1 (membres des listes dans la synchronisation) doit partir en production pour l'essai.
Je fusionne `jalon-1-membres-listes` dans `main` du site et je pousse — ce qui déploie. D'accord ? »
Sur un **oui explicite** seulement :

```bash
cd "/d/Mods Minecraft/EriniumGroupWebsite" && git switch main && git merge --ff-only jalon-1-membres-listes && npx tsc --noEmit && npm test && npm run build && git push
```
Lire la ligne `a..b  main -> main` de la sortie. Attendre que Vercel affiche le déploiement
« Ready ».

- [ ] **Étape 2 (contrôleur) : construire l'installateur de l'essai**

```bash
npm --prefix app run build
cd spike/crates/sky-app && ../../../app/node_modules/.bin/tauri build
```
Relever le chemin et la taille de `SkyShare_0.1.0_x64-setup.exe`.

- [ ] **Étape 3 (propriétaire) : le mode d'emploi, à suivre sur les deux machines**

1. **Fermer tout `sky-probe`** encore ouvert sur la machine. Tant que l'application tourne, ne pas
   lancer `sky-probe` : il lui volerait ses demandes.
2. Copier `SkyShare_0.1.0_x64-setup.exe` sur la machine (clé USB ou partage de fichiers).
3. Lancer l'installateur. Windows affiche « Windows a protégé votre ordinateur » (application non
   signée, spec §6) : **Informations complémentaires → Exécuter quand même**. L'installation se fait
   pour l'utilisateur courant, sans droits d'administrateur.
4. SkyShare s'ouvre. Si Avira (ou un autre pare-feu) demande une autorisation réseau, l'accepter —
   rappel : Avira la retire à chaque nouvelle version (leçon du 23/08).
5. Cliquer **Se connecter avec Discord**. Le navigateur s'ouvre ; se connecter avec **un compte
   Discord différent sur chaque machine**. Revenir à SkyShare : il doit passer seul sur l'écran Amis.
   Chronométrer le temps entre le retour dans l'application et l'affichage de l'écran Amis.
6. Sur la machine A, lire le code ami (l'écran Mon compte n'existe pas encore) :
   `sky-probe code` **une seule fois**, SkyShare n'étant pas en partage. Sur la machine B, le saisir
   dans « Code ami » puis **Ajouter** : le message « Demande envoyée. » s'affiche.
7. Sur A : la demande apparaît dans « Demandes reçues » au plus tard **30 s** plus tard (cadence de
   la fenêtre visible). Chronométrer. **Accepter**. B voit A dans ses amis au plus tard 30 s plus
   tard, avec « 1 appareil ».
8. Fermer la fenêtre SkyShare : l'icône reste près de l'horloge. Clic gauche sur l'icône : la
   fenêtre revient. Clic droit : « Ouvrir SkyShare » et « Quitter ».
9. Relancer SkyShare depuis le menu Démarrer alors qu'il tourne déjà : aucune seconde fenêtre, la
   première revient au premier plan.
10. Redémarrer une des deux machines : SkyShare doit se lancer seul, **caché** (icône seule).
11. **Ne pas cliquer Regarder** : la commande arrive à la tâche 11.

- [ ] **Étape 4 (contrôleur) : consigner**

Dans `tasks/todo.md`, section « Jalon 1 — essai réel 1 », pour chaque point 5 à 10 : réussi ou non,
les deux délais chronométrés, et tout message affiché. Vérifier que l'appareil de chaque machine a
bien été enregistré à la connexion (`sky-probe device list` sur une machine, SkyShare fermé par
« Quitter »). **Tout échec arrête le plan** : diagnostiquer (`superpowers:systematic-debugging`)
avant la tâche 10.

```bash
git add tasks/todo.md
git commit -m "docs: jalon 1 — resultats du premier essai reel"
git push
```

---

## Task 10 : Interface — Listes et Mon compte

**Ce que la tâche livre :** l'écran Listes (à gauche les listes avec couleur, émoji, nombre de
membres et « Nouvelle liste » ; à droite l'édition — nom, couleur, émoji, membres à cocher parmi les
amis, Enregistrer, Supprimer ; la phrase « les listes ne filtrent pas encore les partages ») et
l'écran Mon compte (nom Discord ; code ami avec Copier et Régénérer ; appareils, celui-ci signalé et
non révocable, les autres révocables ; « Lancer SkyShare au démarrage de Windows » ; l'avertissement
sur `sky-probe` (spec §6) ; Se déconnecter) ; les commandes `creer_liste`, `modifier_liste`,
`supprimer_liste`, `definir_membres`, `regenerer_code`, `revoquer_appareil`,
`demarrage_automatique`.

**Point non couvert, tranché ici :** la spec §4 dit le nom de l'appareil « modifiable dans Mon
compte ». **Aucune route du site ne renomme un appareil** (relevé : `devices/route.ts` n'a que
`POST`, `devices/[id]/route.ts` que `DELETE`) et la spec §5 limite le site à une seule
modification. Le renommage est donc **reporté** ; l'écran affiche le nom sans le rendre modifiable.
Signalé au propriétaire.

**Files:**
- Modify: `spike/crates/sky-app/src/noyau.rs`, `spike/crates/sky-app/src/commandes.rs`, `spike/crates/sky-app/src/lib.rs`
- Create: `app/src/ecrans/Listes.tsx`, `app/src/ecrans/Listes.test.tsx`, `app/src/ecrans/MonCompte.tsx`, `app/src/ecrans/MonCompte.test.tsx`
- Modify: `app/src/pont.ts`, `app/src/App.tsx`

**Interfaces:**
- Consumes : `sky_compte::{creer_liste, modifier_liste, supprimer_liste, definir_membres, regenerer_code, revoquer_appareil, ChampsListe, CreationListe, ModificationListe, SuppressionListe, DefinitionMembres}` (T2, T3) ; `Coquille::demarrage_automatique` (T7).
- Produces (Rust) : `Noyau::creer_liste(&self, nom: &str, couleur: Option<&str>, emoji: Option<&str>) -> Result<String, String>`, `modifier_liste(&self, id: i64, nom: &str, couleur: Option<&str>, emoji: Option<&str>) -> Result<String, String>`, `supprimer_liste(&self, id: i64) -> Result<String, String>`, `definir_membres(&self, id: i64, membres: &[i64]) -> Result<String, String>`, `regenerer_code(&self) -> Result<String, String>`, `revoquer_appareil(&self, id: i64) -> Result<String, String>`, `demarrage_automatique(&self, actif: bool) -> Result<(), String>` ; commandes Tauri de mêmes noms (`couleur`/`emoji` : `Option<String>`, `membres` : `Vec<i64>`).
- Produces (interface) : `pont.{creerListe(nom, couleur, emoji), modifierListe(id, nom, couleur, emoji), supprimerListe(id), definirMembres(id, membres), regenererCode(), revoquerAppareil(id), demarrageAutomatique(actif)}` ; composants `Listes({ instantane })`, `MonCompte({ instantane })` ; `codeAffiche(code: string | null): string`.

- [ ] **Étape 1 : écrire les tests du noyau qui échouent**

Dans le module `tests` de `noyau.rs` :

```rust
    #[test]
    fn on_ne_revoque_pas_l_appareil_qu_on_utilise() {
        // Révoquer l'appareil courant révoquerait aussi la session de cette
        // machine (`revoquerAppareil`). Neutralisation : retirer la
        // comparaison à l'appareil du coffre — le double le révoque.
        let c = contexte("sky-test-app-revoquer-soi", true);
        c.noyau.demarrer(); // enregistre l'appareil de la machine
        let courant = c.noyau.coffre().identifiant_appareil().unwrap().unwrap();
        assert!(c.noyau.revoquer_appareil(courant).is_err());
        assert!(c.serveur.etat_mut().appareils_revoques.is_empty());
    }

    #[test]
    fn un_code_regenere_se_voit_aussitot() {
        // Le code n'entre pas dans la version : seule la resynchronisation
        // complète le montre. Neutralisation : comme pour le retrait d'un ami.
        let c = contexte("sky-test-app-code", true);
        c.noyau.synchroniser().unwrap();
        c.noyau.regenerer_code().unwrap();
        assert_eq!(c.noyau.instantane().code.as_deref(), Some("REGENAA2"));
    }

    #[test]
    fn le_demarrage_automatique_passe_par_la_coquille_et_se_voit() {
        // Neutralisation : ne pas mettre à jour `demarrage_automatique` — la
        // case resterait cochée après l'avoir décochée.
        let c = contexte("sky-test-app-demarrage", true);
        c.noyau.definir_demarrage_automatique_connu(true);
        c.noyau.demarrage_automatique(false).unwrap();
        assert_eq!(*c.coquille.demarrages.lock().unwrap(), vec![false]);
        assert!(!c.noyau.instantane().demarrage_automatique);
    }
```

```bash
cd spike && cargo test -p sky-app
```
Attendu : ÉCHEC à la compilation.

- [ ] **Étape 2 : implémenter**

Dans `noyau.rs`, ajouter aux `use` : `use sky_compte::{ChampsListe, CreationListe, DefinitionMembres, ModificationListe, SuppressionListe};`,
après les constantes :

```rust
const MESSAGE_NOM_PRIS: &str = "Une liste porte déjà ce nom.";
const MESSAGE_LISTE_DISPARUE: &str = "Cette liste n'existe plus.";
```

et dans `impl Noyau` :

```rust
    pub fn creer_liste(&self, nom: &str, couleur: Option<&str>, emoji: Option<&str>) -> Result<String, String> {
        self.exiger_connexion()?;
        let issue = sky_compte::creer_liste(&self.config, &self.coffre, nom, couleur, emoji)
            .map_err(|e| self.apres_erreur(&e))?;
        self.apres_commande();
        match issue {
            CreationListe::Creee(_) => Ok("Liste créée.".to_string()),
            CreationListe::NomDejaPris => Err(MESSAGE_NOM_PRIS.to_string()),
        }
    }

    /// L'écran d'édition montre toujours la liste entière : il envoie les
    /// trois champs, `None` remettant couleur ou émoji à rien.
    pub fn modifier_liste(&self, id: i64, nom: &str, couleur: Option<&str>, emoji: Option<&str>) -> Result<String, String> {
        self.exiger_connexion()?;
        let champs = ChampsListe { nom: Some(nom), couleur: Some(couleur), emoji: Some(emoji) };
        let issue = sky_compte::modifier_liste(&self.config, &self.coffre, id, &champs)
            .map_err(|e| self.apres_erreur(&e))?;
        self.apres_commande();
        match issue {
            ModificationListe::Modifiee => Ok("Liste enregistrée.".to_string()),
            ModificationListe::Introuvable => Err(MESSAGE_LISTE_DISPARUE.to_string()),
            ModificationListe::NomDejaPris => Err(MESSAGE_NOM_PRIS.to_string()),
        }
    }

    pub fn supprimer_liste(&self, id: i64) -> Result<String, String> {
        self.exiger_connexion()?;
        let issue = sky_compte::supprimer_liste(&self.config, &self.coffre, id).map_err(|e| self.apres_erreur(&e))?;
        self.apres_commande();
        match issue {
            SuppressionListe::Supprimee => Ok("Liste supprimée.".to_string()),
            SuppressionListe::Introuvable => Err(MESSAGE_LISTE_DISPARUE.to_string()),
        }
    }

    /// `membres` : identifiants d'UTILISATEUR des amis cochés.
    pub fn definir_membres(&self, id: i64, membres: &[i64]) -> Result<String, String> {
        self.exiger_connexion()?;
        let issue = sky_compte::definir_membres(&self.config, &self.coffre, id, membres)
            .map_err(|e| self.apres_erreur(&e))?;
        self.apres_commande();
        match issue {
            DefinitionMembres::Definis => Ok("Membres enregistrés.".to_string()),
            DefinitionMembres::Refuses => {
                Err("Un des membres n'est plus ton ami, ou la liste n'existe plus : rien n'a changé.".to_string())
            }
        }
    }

    pub fn regenerer_code(&self) -> Result<String, String> {
        self.exiger_connexion()?;
        let code = sky_compte::regenerer_code(&self.config, &self.coffre).map_err(|e| self.apres_erreur(&e))?;
        self.apres_commande();
        // Le site rend 8 caractères ASCII de l'alphabet ; si ce n'était pas le
        // cas, le découpage ci-dessous paniquerait — on montre le code brut.
        if code.len() != 8 || !code.is_ascii() {
            return Ok(format!("Nouveau code : {code}. L'ancien ne fonctionne plus."));
        }
        Ok(format!("Nouveau code : SKY-{}-{}. L'ancien ne fonctionne plus.", &code[..4], &code[4..]))
    }

    /// Jamais l'appareil de cette machine : sa révocation révoquerait aussi
    /// la session courante (`revoquerAppareil`, site).
    pub fn revoquer_appareil(&self, id: i64) -> Result<String, String> {
        self.exiger_connexion()?;
        let courant = self.coffre.identifiant_appareil().map_err(|e| message_erreur(&e))?;
        if courant == Some(id) {
            return Err("C'est l'appareil que tu utilises : il ne peut pas se révoquer lui-même.".to_string());
        }
        sky_compte::revoquer_appareil(&self.config, &self.coffre, id).map_err(|e| self.apres_erreur(&e))?;
        self.apres_commande();
        Ok("Appareil révoqué.".to_string())
    }

    pub fn demarrage_automatique(&self, actif: bool) -> Result<(), String> {
        self.branchements.coquille.demarrage_automatique(actif)?;
        self.donnees().demarrage_automatique = actif;
        self.publier();
        Ok(())
    }
```

Dans `commandes.rs` :

```rust
#[tauri::command]
pub async fn creer_liste(
    noyau: State<'_, Arc<Noyau>>,
    nom: String,
    couleur: Option<String>,
    emoji: Option<String>,
) -> Result<String, String> {
    sur_un_fil(noyau.inner(), move |n| n.creer_liste(&nom, couleur.as_deref(), emoji.as_deref())).await?
}

#[tauri::command]
pub async fn modifier_liste(
    noyau: State<'_, Arc<Noyau>>,
    id: i64,
    nom: String,
    couleur: Option<String>,
    emoji: Option<String>,
) -> Result<String, String> {
    sur_un_fil(noyau.inner(), move |n| n.modifier_liste(id, &nom, couleur.as_deref(), emoji.as_deref())).await?
}

#[tauri::command]
pub async fn supprimer_liste(noyau: State<'_, Arc<Noyau>>, id: i64) -> Result<String, String> {
    sur_un_fil(noyau.inner(), move |n| n.supprimer_liste(id)).await?
}

#[tauri::command]
pub async fn definir_membres(noyau: State<'_, Arc<Noyau>>, id: i64, membres: Vec<i64>) -> Result<String, String> {
    sur_un_fil(noyau.inner(), move |n| n.definir_membres(id, &membres)).await?
}

#[tauri::command]
pub async fn regenerer_code(noyau: State<'_, Arc<Noyau>>) -> Result<String, String> {
    sur_un_fil(noyau.inner(), |n| n.regenerer_code()).await?
}

#[tauri::command]
pub async fn revoquer_appareil(noyau: State<'_, Arc<Noyau>>, id: i64) -> Result<String, String> {
    sur_un_fil(noyau.inner(), move |n| n.revoquer_appareil(id)).await?
}

#[tauri::command]
pub async fn demarrage_automatique(noyau: State<'_, Arc<Noyau>>, actif: bool) -> Result<(), String> {
    sur_un_fil(noyau.inner(), move |n| n.demarrage_automatique(actif)).await?
}
```

`lib.rs`, `generate_handler!` gagne les sept commandes.

```bash
cd spike && cargo test -p sky-app && cargo clippy --all-targets -- -D warnings
```
Attendu : PASS. Neutraliser les trois tests ; rétablir.

- [ ] **Étape 3 : écrire les tests d'écrans qui échouent**

Dans `app/src/pont.ts`, ajouter à l'objet `pont` :

```ts
  creerListe: (nom: string, couleur: string | null, emoji: string | null) =>
    invoke<string>("creer_liste", { nom, couleur, emoji }),
  modifierListe: (id: number, nom: string, couleur: string | null, emoji: string | null) =>
    invoke<string>("modifier_liste", { id, nom, couleur, emoji }),
  supprimerListe: (id: number) => invoke<string>("supprimer_liste", { id }),
  /** `membres` : identifiants d'UTILISATEUR (AmiVue.id). */
  definirMembres: (id: number, membres: number[]) => invoke<string>("definir_membres", { id, membres }),
  regenererCode: () => invoke<string>("regenerer_code"),
  revoquerAppareil: (id: number) => invoke<string>("revoquer_appareil", { id }),
  demarrageAutomatique: (actif: boolean) => invoke<void>("demarrage_automatique", { actif }),
```

`app/src/ecrans/Listes.test.tsx` :

```tsx
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { pont } from "../pont";
import { instantaneDeTest } from "../test/fabriques";
import { Listes } from "./Listes";

vi.mock("../pont", () => ({
  pont: { creerListe: vi.fn(), modifierListe: vi.fn(), supprimerListe: vi.fn(), definirMembres: vi.fn() },
}));

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(pont.modifierListe).mockResolvedValue("Liste enregistrée.");
  vi.mocked(pont.definirMembres).mockResolvedValue("Membres enregistrés.");
});

const AMIS = [
  { id: 2, friendshipId: 12, nom: "Bob", appareils: 1 },
  { id: 3, friendshipId: 13, nom: "Carole", appareils: 0 },
];
const JEU = { id: 7, nom: "Jeu", couleur: "#C4664A", emoji: "🎮", membres: [2] };

describe("Listes", () => {
  it("dit que les listes ne filtrent pas encore les partages", () => {
    render(<Listes instantane={instantaneDeTest()} />);
    expect(screen.getByText(/ne filtrent pas encore les partages/)).toBeInTheDocument();
  });

  it("les cases cochées sont les membres, et l'enregistrement envoie des identifiants d'utilisateur", async () => {
    // Spec §5 : sans `membres` dans la synchronisation, aucune case ne serait
    // cochée. Neutralisations : initialiser les cases à vide (Bob décoché) ;
    // envoyer `friendshipId` (appel avec [12, 13]).
    render(<Listes instantane={instantaneDeTest({ amis: AMIS, listes: [JEU] })} />);
    await userEvent.click(screen.getByRole("button", { name: /Jeu/ }));
    const edition = within(screen.getByRole("form", { name: "Édition de la liste" }));
    expect(edition.getByRole("checkbox", { name: "Bob" })).toBeChecked();
    expect(edition.getByRole("checkbox", { name: "Carole" })).not.toBeChecked();
    await userEvent.click(edition.getByRole("checkbox", { name: "Carole" }));
    await userEvent.click(edition.getByRole("button", { name: "Enregistrer" }));
    expect(pont.modifierListe).toHaveBeenCalledWith(7, "Jeu", "#c4664a", "🎮");
    expect(pont.definirMembres).toHaveBeenCalledWith(7, [2, 3]);
  });
});
```

(`<input type="color">` rend sa valeur en minuscules : `#c4664a`. Le site accepte les deux casses.)

`app/src/ecrans/MonCompte.test.tsx` :

```tsx
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { pont } from "../pont";
import { instantaneDeTest } from "../test/fabriques";
import { MonCompte, codeAffiche } from "./MonCompte";

vi.mock("../pont", () => ({
  pont: { regenererCode: vi.fn(), revoquerAppareil: vi.fn(), demarrageAutomatique: vi.fn(), deconnexion: vi.fn() },
}));

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(pont.revoquerAppareil).mockResolvedValue("Appareil révoqué.");
  vi.mocked(pont.demarrageAutomatique).mockResolvedValue(undefined);
});

const APPAREILS = [
  { id: 4, nom: "BUREAU", courant: true, revoque: false },
  { id: 5, nom: "PORTABLE", courant: false, revoque: false },
];

describe("Mon compte", () => {
  it("l'appareil courant n'est pas révocable, les autres le sont", async () => {
    // Neutralisation : afficher Révoquer aussi pour `courant`.
    render(<MonCompte instantane={instantaneDeTest({ appareils: APPAREILS })} />);
    const bureau = within(screen.getByText("BUREAU").closest("li")!);
    expect(bureau.queryByRole("button", { name: "Révoquer" })).toBeNull();
    expect(bureau.getByText("cet appareil")).toBeInTheDocument();
    await userEvent.click(within(screen.getByText("PORTABLE").closest("li")!).getByRole("button", { name: "Révoquer" }));
    expect(pont.revoquerAppareil).toHaveBeenCalledWith(5);
  });

  it("la case de démarrage envoie l'inverse de l'état affiché", async () => {
    render(<MonCompte instantane={instantaneDeTest({ demarrageAutomatique: true })} />);
    const caseDemarrage = screen.getByRole("checkbox", { name: "Lancer SkyShare au démarrage de Windows" });
    expect(caseDemarrage).toBeChecked();
    await userEvent.click(caseDemarrage);
    expect(pont.demarrageAutomatique).toHaveBeenCalledWith(false);
  });

  it("prévient que sky-probe vole les demandes pendant que l'application tourne", () => {
    // Spec §6 : « L'application le dit dans Mon compte ».
    render(<MonCompte instantane={instantaneDeTest()} />);
    expect(screen.getByText(/sky-probe/)).toBeInTheDocument();
  });

  it("le code s'affiche sous la forme que le site accepte", () => {
    expect(codeAffiche("ABCD2345")).toBe("SKY-ABCD-2345");
    expect(codeAffiche(null)).toBe("—");
  });
});
```

```bash
npm --prefix app test
```
Attendu : FAIL — `./Listes`, `./MonCompte` introuvables.

- [ ] **Étape 4 : écrire les écrans**

`app/src/ecrans/Listes.tsx` :

```tsx
import { useState } from "react";
import { pont } from "../pont";
import type { AmiVue, Instantane, ListeVue } from "../types";

export function Listes({ instantane }: { instantane: Instantane }) {
  const [choisie, setChoisie] = useState<number | "nouvelle" | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const liste = typeof choisie === "number" ? (instantane.listes.find((l) => l.id === choisie) ?? null) : null;
  return (
    <section className="flex flex-col gap-4">
      <h1 className="font-titre text-4xl">Listes</h1>
      <p className="text-texte-3">
        Les listes ne filtrent pas encore les partages : tous tes amis peuvent te demander à regarder.
      </p>
      <div className="flex gap-6">
        <ul aria-label="Mes listes" className="flex w-64 flex-col gap-1">
          {instantane.listes.map((l) => (
            <li key={l.id}>
              <button
                type="button"
                onClick={() => setChoisie(l.id)}
                className="flex w-full items-center gap-2 rounded-md px-3 py-2 hover:bg-surface-haute"
              >
                <span aria-hidden="true" className="h-3 w-3 rounded-full" style={{ background: l.couleur ?? "transparent" }} />
                <span>{l.emoji}</span>
                <span className="flex-1 text-left">{l.nom}</span>
                <span className="text-texte-3">{l.membres.length}</span>
              </button>
            </li>
          ))}
          <li>
            <button type="button" onClick={() => setChoisie("nouvelle")} className="px-3 py-2 text-accent">
              Nouvelle liste
            </button>
          </li>
        </ul>
        {(choisie === "nouvelle" || liste !== null) && (
          <EditeurListe
            key={liste?.id ?? "nouvelle"}
            liste={liste}
            amis={instantane.amis}
            signaler={setMessage}
            fermer={() => setChoisie(null)}
          />
        )}
      </div>
      {message && (
        <p role="status" className="text-texte-2">
          {message}
        </p>
      )}
    </section>
  );
}

function EditeurListe(props: {
  liste: ListeVue | null;
  amis: AmiVue[];
  signaler: (message: string) => void;
  fermer: () => void;
}) {
  const { liste } = props;
  const [nom, setNom] = useState(liste?.nom ?? "");
  const [couleur, setCouleur] = useState((liste?.couleur ?? "#C4664A").toLowerCase());
  const [emoji, setEmoji] = useState(liste?.emoji ?? "");
  const [membres, setMembres] = useState<Set<number>>(new Set(liste?.membres ?? []));

  function basculer(id: number) {
    setMembres((avant) => {
      const apres = new Set(avant);
      if (apres.has(id)) apres.delete(id);
      else apres.add(id);
      return apres;
    });
  }

  async function enregistrer() {
    const emojiEnvoye = emoji === "" ? null : emoji;
    try {
      if (liste) {
        await pont.modifierListe(liste.id, nom, couleur, emojiEnvoye);
        props.signaler(await pont.definirMembres(liste.id, [...membres].sort((a, b) => a - b)));
      } else {
        props.signaler(await pont.creerListe(nom, couleur, emojiEnvoye));
        props.fermer();
      }
    } catch (erreur) {
      props.signaler(String(erreur));
    }
  }

  async function supprimer() {
    if (!liste) return;
    try {
      props.signaler(await pont.supprimerListe(liste.id));
      props.fermer();
    } catch (erreur) {
      props.signaler(String(erreur));
    }
  }

  return (
    <form
      aria-label="Édition de la liste"
      className="flex flex-1 flex-col gap-3 rounded-md bg-surface p-4"
      onSubmit={(e) => {
        e.preventDefault();
        void enregistrer();
      }}
    >
      <label className="flex flex-col gap-1">
        Nom
        <input value={nom} onChange={(e) => setNom(e.target.value)} maxLength={40} className="rounded-md border border-bordure bg-fond px-3 py-2" />
      </label>
      <label className="flex items-center gap-2">
        Couleur
        <input type="color" value={couleur} onChange={(e) => setCouleur(e.target.value)} />
      </label>
      <label className="flex flex-col gap-1">
        Émoji
        <input value={emoji} onChange={(e) => setEmoji(e.target.value)} className="rounded-md border border-bordure bg-fond px-3 py-2" />
      </label>
      {liste && (
        <fieldset className="flex flex-col gap-1">
          <legend className="text-texte-2">Membres</legend>
          {props.amis.map((ami) => (
            <label key={ami.id} className="flex items-center gap-2">
              <input type="checkbox" checked={membres.has(ami.id)} onChange={() => basculer(ami.id)} />
              {ami.nom}
            </label>
          ))}
        </fieldset>
      )}
      <div className="flex gap-2">
        <button type="submit" className="rounded-md bg-accent px-4 py-2 text-fond">
          Enregistrer
        </button>
        {liste && (
          <button type="button" onClick={() => void supprimer()} className="rounded-md px-4 py-2 text-alerte">
            Supprimer
          </button>
        )}
      </div>
    </form>
  );
}
```

`app/src/ecrans/MonCompte.tsx` :

```tsx
import { useState } from "react";
import { pont } from "../pont";
import type { Instantane } from "../types";

/** « ABCD2345 » → « SKY-ABCD-2345 », forme que le site normalise (`normaliserCode`). */
export function codeAffiche(code: string | null): string {
  return code ? `SKY-${code.slice(0, 4)}-${code.slice(4)}` : "—";
}

export function MonCompte({ instantane }: { instantane: Instantane }) {
  const [message, setMessage] = useState<string | null>(null);

  async function executer(action: () => Promise<string | void>) {
    try {
      const retour = await action();
      setMessage(typeof retour === "string" ? retour : null);
    } catch (erreur) {
      setMessage(String(erreur));
    }
  }

  return (
    <section className="flex max-w-2xl flex-col gap-6">
      <h1 className="font-titre text-4xl">Mon compte</h1>
      <p>
        Connecté en tant que <strong>{instantane.nom ?? "…"}</strong>
      </p>
      <section aria-label="Code ami" className="flex items-center gap-3">
        <span className="text-texte-2">Ton code ami</span>
        <code className="rounded-md bg-surface px-3 py-1">{codeAffiche(instantane.code)}</code>
        <button type="button" onClick={() => void navigator.clipboard.writeText(codeAffiche(instantane.code)).then(() => setMessage("Code copié."))}>
          Copier
        </button>
        <button type="button" onClick={() => void executer(() => pont.regenererCode())}>
          Régénérer
        </button>
      </section>
      <section aria-label="Appareils" className="flex flex-col gap-2">
        <h2 className="text-texte-2">Appareils</h2>
        <ul className="flex flex-col gap-2">
          {instantane.appareils.map((appareil) => (
            <li key={appareil.id} className="flex items-center gap-3 rounded-md bg-surface px-3 py-2">
              <span className="flex-1">{appareil.nom}</span>
              {appareil.courant ? (
                <span className="text-succes">cet appareil</span>
              ) : appareil.revoque ? (
                <span className="text-texte-3">révoqué</span>
              ) : (
                <button type="button" onClick={() => void executer(() => pont.revoquerAppareil(appareil.id))}>
                  Révoquer
                </button>
              )}
            </li>
          ))}
        </ul>
      </section>
      <label className="flex items-center gap-2">
        <input
          type="checkbox"
          checked={instantane.demarrageAutomatique}
          onChange={() => void executer(() => pont.demarrageAutomatique(!instantane.demarrageAutomatique))}
        />
        Lancer SkyShare au démarrage de Windows
      </label>
      <p className="text-texte-3">
        Tant que SkyShare tourne, n'utilise pas <code>sky-probe</code> sur cette machine : il lui volerait les
        demandes de partage.
      </p>
      <button type="button" className="self-start text-alerte" onClick={() => void executer(() => pont.deconnexion())}>
        Se déconnecter
      </button>
      {message && (
        <p role="status" className="text-texte-2">
          {message}
        </p>
      )}
    </section>
  );
}
```

Dans `app/src/App.tsx`, importer `Listes` et `MonCompte` et remplacer l'expression enfant de
`Disposition` par :

```tsx
      {ecran === "amis" && <Amis instantane={instantane} />}
      {ecran === "listes" && <Listes instantane={instantane} />}
      {ecran === "compte" && <MonCompte instantane={instantane} />}
```
(retirer alors la constante `TITRES`, devenue inutile ; le test `App.test.tsx` cherche le titre
« Listes », que l'écran Listes porte.)

- [ ] **Étape 5 : lancer, neutraliser, construire**

```bash
npm --prefix app test && npm --prefix app run build
```
Attendu : 14 tests verts. Neutraliser chaque test selon son commentaire ; rétablir.

- [ ] **Étape 6 : commiter et pousser**

```bash
git add spike/crates/sky-app/src/noyau.rs
git add spike/crates/sky-app/src/commandes.rs
git add spike/crates/sky-app/src/lib.rs
git add app/src/pont.ts
git add app/src/App.tsx
git add app/src/ecrans/Listes.tsx
git add app/src/ecrans/Listes.test.tsx
git add app/src/ecrans/MonCompte.tsx
git add app/src/ecrans/MonCompte.test.tsx
git diff --cached --stat
git commit -m "feat: ecrans Listes et Mon compte, commandes de listes et de compte"
git push
```

---

## Task 11 : Partage dans `sky-app`

**Ce que la tâche livre :** les commandes `partager` (écran choisi), `regarder` (ami désigné par son
identifiant d'utilisateur) et `arreter` ; l'événement `partage` ; l'icône près de l'horloge qui
**change tant que dure un partage** (attente comprise : un ami peut se connecter à tout moment) ; le
bouton désactivé sans carte NVIDIA ; **aucune (re)connexion pendant un partage ou une attente**
(spec §6) ; la synchronisation du `Noyau` — la seule de l'application — prêtée à la négociation.

**Détection de NVENC (relevée, pas supposée).** `sky-probe hw` appelle
`sky_encode::probe_hardware()` puis `pick_best(&caps, true)` (`cmd_hw.rs`, `caps.rs:40,113`).
`probe_hardware` ouvre `nvcuda.dll` par `cudarc` 0.16.6, qui **panique** si la bibliothèque est
absente (`cudarc-0.16.6/src/lib.rs:108`, `panic!("Unable to dynamically load …")`) au lieu de rendre
une erreur. Sur une machine sans pilote NVIDIA, un appel nu fermerait l'application au lancement.
`detecter_nvenc` l'appelle sous `std::panic::catch_unwind`.

**Écrans (relevé).** `WgcCapture::new(index)` prend le rang dans `EnumDisplayMonitors`
(`sky-capture/src/wgc.rs:45,220`). `app.available_monitors()` de Tauri (tao 0.35.3,
`platform_impl/windows/monitor.rs:103`) énumère par le même appel, dans le même ordre (`push_back`
dans le rappel) : le rang de la liste Tauri est donc celui qu'attend la capture. L'écran principal
est reconnu par son nom (`primary_monitor()`).

**Files:**
- Modify: `spike/crates/sky-app/Cargo.toml` (`sky-encode`, `anyhow`), `spike/Cargo.lock`
- Create: `spike/crates/sky-app/src/partage.rs`, `spike/crates/sky-app/icons/partage.svg`, `spike/crates/sky-app/icons/partage/32x32.png` (généré)
- Modify: `spike/crates/sky-app/src/{materiel,noyau,coquille,essais,commandes,lib}.rs`

**Interfaces:**
- Consumes : `sky_partage::{heberger, regarder, Arret, Designation, ErreurPartage, Evenement, Fin, Mesures, ParametresHote, ParametresSpectateur, SourceImages, Diagnostic}`, `sky_partage::rendez_vous::FENETRE_HOTE` (T4, T5) ; `sky_encode::{probe_hardware, pick_best, Codec, EncoderCaps, EncodeError}`.
- Produces :
  - `partage.rs` : `PLAFOND_MBPS: u32 = 30`, `PLANCHER_MBPS: u32 = 10` ; `trait Partageur: Send + Sync { fn heberger(&self, noyau: &Noyau, codec: Codec, ecran: usize, arret: &Arret, evenements: &mut dyn FnMut(Evenement)) -> Result<Fin, ErreurPartage>; fn regarder(&self, noyau: &Noyau, ami: i64, arret: &Arret, evenements: &mut dyn FnMut(Evenement)) -> Result<Fin, ErreurPartage>; }` ; `PartageurReel` ; `maintenant_ms() -> u64` ; `fin_vue(issue: &Result<Fin, ErreurPartage>) -> FinVue` ; `appliquer(actuel: &PartageVue, evenement: &Evenement, maintenant_ms: u64, etat: Option<&Etat>) -> Option<PartageVue>`.
  - `materiel.rs` : `detecter_nvenc(sonde: impl FnOnce() -> Result<EncoderCaps, EncodeError> + UnwindSafe) -> Option<Codec>`.
  - `coquille.rs` : `Coquille::icone_partage(&self, actif: bool)`.
  - `noyau.rs` : `Branchements` gagne `partageur: Box<dyn Partageur>` ; `Donnees::nvenc` devient `Option<Codec>`, `Donnees` gagne `arret: Option<Arret>` ; `MESSAGE_SANS_NVIDIA` ; `Noyau::{definir_nvenc(&self, Option<Codec>), partager(self: &Arc<Self>, ecran: usize) -> Result<(), String>, regarder(self: &Arc<Self>, ami: i64) -> Result<(), String>, arreter(&self)}` ; `pub(crate) sur_evenement(&self, &Evenement)`.
  - Commandes Tauri `partager(ecran: usize)`, `regarder(ami: i64)`, `arreter()` → `Result<(), String>`.
  - `essais.rs` : `PartageurFactice` ; `CoquilleEspion::icones`.

- [ ] **Étape 1 : écrire les tests purs qui échouent**

Créer `spike/crates/sky-app/src/partage.rs` avec seulement :

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use sky_compte::{Ami, AppareilDAmi};
    use sky_partage::Diagnostic;
    use std::time::Duration;

    fn diagnostic(ice_connecte: bool) -> Diagnostic {
        Diagnostic {
            ice_connecte,
            emis: 10,
            recus: 0,
            erreurs: 0,
            vers_local: 0,
            vers_internet: 10,
            erreurs_socket: 0,
            delai: Duration::from_secs(25),
        }
    }

    #[test]
    fn chaque_echec_de_la_spec_a_sa_cause() {
        // Spec §4, « Échecs, tous en clair ». Neutralisations : faire rendre
        // `Autre` à chacune des quatre branches, une à la fois.
        assert_eq!(fin_vue(&Ok(Fin::PasDeReponse { nom: "Bob".into() })), FinVue::PasEnPartage { ami: "Bob".into() });
        assert_eq!(fin_vue(&Ok(Fin::EtablissementEchoue(diagnostic(false)))), FinVue::ReseauBloque);
        assert_eq!(fin_vue(&Ok(Fin::TamponSature { morceau: 3, morceaux: 9 })), FinVue::TropLente);
        assert_eq!(fin_vue(&Err(ErreurPartage::Compte(ErreurCompte::Refuse))), FinVue::SessionExpiree);
    }

    #[test]
    fn une_negociation_chiffree_ratee_n_accuse_pas_le_reseau() {
        // ICE a abouti : le réseau n'est pas en cause (diagnostic du C2).
        // Neutralisation : ignorer `ice_connecte` dans `fin_vue`.
        assert!(matches!(fin_vue(&Ok(Fin::EtablissementEchoue(diagnostic(true)))), FinVue::Autre { .. }));
    }

    fn etat_avec_bob() -> Etat {
        Etat {
            version: 1,
            code: "ABCD2345".into(),
            amis: vec![Ami {
                id: 2,
                friendship_id: 12,
                discord_name: "Bob".into(),
                appareils: vec![AppareilDAmi { id: 42, public_key: String::new() }],
            }],
            demandes: Vec::new(),
            listes: Vec::new(),
            appareils: Vec::new(),
            enveloppes: Vec::new(),
        }
    }

    #[test]
    fn la_demande_recue_nomme_l_ami_proprietaire_de_l_appareil() {
        // « Le panneau central montre qui regarde » (spec §4).
        // Neutralisation : `spectateur: None`.
        let actuel = PartageVue::Disponible { debut_ms: 0, fenetre_s: 1800, ecran: 1 };
        let demande = Evenement::DemandeRecue { expediteur_device_id: 42, apres: Duration::from_secs(3), synchronisations: 2 };
        assert_eq!(
            appliquer(&actuel, &demande, 5_000, Some(&etat_avec_bob())),
            Some(PartageVue::Diffuse { spectateur: Some("Bob".into()), depuis_ms: 5_000, debit_mbps: 0.0, rtt_ms: 0.0, ecran: 1 })
        );
    }

    #[test]
    fn la_connexion_du_spectateur_garde_sa_duree_et_les_mesures_s_y_ajoutent() {
        let demande = PartageVue::Demande { ami: "Bob".into(), debut_ms: 0 };
        let connecte = Evenement::Connecte { en: Duration::from_millis(600), depuis_le_lancement: Some(Duration::from_secs(7)) };
        let regarde = appliquer(&demande, &connecte, 9_000, None).unwrap();
        let mesure = Evenement::Mesures(Mesures::Reception { debit_mbps: 12.4, images_par_s: 107, gigue_ms: 5.0 });
        assert_eq!(
            appliquer(&regarde, &mesure, 10_000, None),
            Some(PartageVue::Regarde {
                ami: "Bob".into(),
                connecte_en_s: 0.6,
                debit_mbps: 12.4,
                images_par_s: 107,
                gigue_ms: 5.0,
                depuis_ms: 9_000,
            })
        );
    }
}
```

Dans le module `tests` de `materiel.rs`, ajouter :

```rust
    use sky_encode::{Codec, EncoderCaps};

    #[test]
    fn une_sonde_qui_panique_rend_aucune_carte() {
        // `cudarc` panique quand nvcuda.dll est absente. Neutralisation :
        // appeler `sonde()` sans `catch_unwind` — le test panique.
        assert_eq!(detecter_nvenc(|| panic!("Unable to dynamically load the \"cuda\" shared library")), None);
    }

    #[test]
    fn une_carte_hevc_444_est_retenue_pour_le_texte() {
        let caps = EncoderCaps { gpu_name: "RTX".into(), codecs: vec![Codec::H264_420, Codec::Hevc444] };
        assert_eq!(detecter_nvenc(move || Ok(caps)), Some(Codec::Hevc444));
    }
```

Dans `spike/crates/sky-app/Cargo.toml`, `[dependencies]` gagne :
```toml
anyhow.workspace = true
sky-encode = { version = "0.0.0", path = "../sky-encode" }
```
Déclarer `pub mod partage;` dans `lib.rs`.

```bash
cd spike && cargo test -p sky-app
```
Attendu : ÉCHEC à la compilation (`fin_vue`, `appliquer`, `detecter_nvenc` introuvables).

- [ ] **Étape 2 : implémenter `partage.rs` et `detecter_nvenc`**

Au-dessus du module de tests de `partage.rs` :

```rust
//! Le partage dans l'application (spec §3, §4) : `sky-partage` branché sur la
//! synchronisation du `Noyau` — la SEULE de l'application —, ses événements
//! traduits en `PartageVue`, ses fins en causes affichables.

use std::time::{SystemTime, UNIX_EPOCH};

use sky_compte::{ErreurCompte, Etat};
use sky_encode::Codec;
use sky_partage::{
    Arret, Designation, ErreurPartage, Evenement, Fin, Mesures, ParametresHote, ParametresSpectateur, SourceImages,
};

use crate::noyau::Noyau;
use crate::vue::{FinVue, PartageVue};

/// Plafond et plancher du débit : les valeurs par défaut de `sky-probe host`,
/// celles de l'essai réel du C2.
pub const PLAFOND_MBPS: u32 = 30;
pub const PLANCHER_MBPS: u32 = 10;

pub trait Partageur: Send + Sync {
    fn heberger(
        &self,
        noyau: &Noyau,
        codec: Codec,
        ecran: usize,
        arret: &Arret,
        evenements: &mut dyn FnMut(Evenement),
    ) -> Result<Fin, ErreurPartage>;

    fn regarder(
        &self,
        noyau: &Noyau,
        ami: i64,
        arret: &Arret,
        evenements: &mut dyn FnMut(Evenement),
    ) -> Result<Fin, ErreurPartage>;
}

/// `sky-partage` pour de vrai. La fermeture de synchronisation ignore le
/// précédent que lui passe `interroger` : le `Noyau` tient le sien, le même.
pub struct PartageurReel;

impl Partageur for PartageurReel {
    fn heberger(
        &self,
        noyau: &Noyau,
        codec: Codec,
        ecran: usize,
        arret: &Arret,
        evenements: &mut dyn FnMut(Evenement),
    ) -> Result<Fin, ErreurPartage> {
        sky_partage::heberger(
            noyau.config(),
            noyau.coffre(),
            |_| noyau.synchroniser(),
            ParametresHote {
                codec,
                plafond_mbps: PLAFOND_MBPS,
                plancher_mbps: PLANCHER_MBPS,
                moniteur: ecran,
                source: SourceImages::Ecran,
                duree_max: None,
            },
            arret,
            evenements,
        )
    }

    fn regarder(
        &self,
        noyau: &Noyau,
        ami: i64,
        arret: &Arret,
        evenements: &mut dyn FnMut(Evenement),
    ) -> Result<Fin, ErreurPartage> {
        // Aucun puits : le flux est mesuré puis JETÉ (spec D2).
        sky_partage::regarder(
            noyau.config(),
            noyau.coffre(),
            |_| noyau.synchroniser(),
            ParametresSpectateur { ami: Designation::Identifiant(ami), duree_max: None },
            || Ok(None),
            arret,
            evenements,
        )
    }
}

pub fn maintenant_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

/// La cause affichée d'une fin de partage (spec §4, « Échecs, tous en clair »).
///
/// `ReseauBloque` n'est rendu que si ICE n'a trouvé AUCUN chemin. Son texte
/// (« Aucune connexion directe n'a pu s'établir entre vos deux réseaux »)
/// énonce ce qui a été constaté, pas une cause supposée — arbitrage du
/// contrôleur contre l'ancien « Ton réseau bloque la connexion directe », qui
/// accusait un réseau sans l'avoir mesuré. Même prudence que le diagnostic du C2.
pub fn fin_vue(issue: &Result<Fin, ErreurPartage>) -> FinVue {
    match issue {
        Ok(Fin::Arrete) | Ok(Fin::DureeEcoulee(_)) => FinVue::Arrete,
        Ok(Fin::PasDeReponse { nom }) => FinVue::PasEnPartage { ami: nom.clone() },
        Ok(Fin::EtablissementEchoue(diagnostic)) if !diagnostic.ice_connecte => FinVue::ReseauBloque,
        Ok(Fin::EtablissementEchoue(_)) => FinVue::Autre {
            message: "Les deux machines se sont trouvées, mais la négociation chiffrée n'a pas abouti.".into(),
        },
        Ok(Fin::TamponSature { .. }) => FinVue::TropLente,
        Ok(Fin::AucuneDemande) => FinVue::AucuneDemande,
        Ok(Fin::AucunAppareilLocal) => FinVue::Autre {
            message: "Cette machine n'a pas d'appareil enregistré : reconnecte-toi.".into(),
        },
        Ok(Fin::AucunAppareilChezLAmi { nom }) => FinVue::Autre { message: format!("{nom} n'a aucun appareil enregistré.") },
        Ok(Fin::DemandeRefusee { nom, .. }) => FinVue::Autre { message: format!("Le site a refusé la demande pour {nom}.") },
        Ok(Fin::ReponseRefusee) => FinVue::Autre { message: "Le site a refusé la réponse : ton ami ne l'a pas reçue.".into() },
        Ok(Fin::NegociationRompue(raison)) | Ok(Fin::LienTombe(raison)) => {
            FinVue::Autre { message: format!("Connexion interrompue : {raison}") }
        }
        Err(ErreurPartage::Compte(ErreurCompte::Refuse)) => FinVue::SessionExpiree,
        Err(ErreurPartage::Compte(e)) => FinVue::Autre { message: crate::noyau::message_erreur(e) },
        Err(ErreurPartage::Autre(e)) => FinVue::Autre { message: e.to_string() },
    }
}

/// Le partage affiché après un événement, ou `None` s'il ne change rien.
pub fn appliquer(
    actuel: &PartageVue,
    evenement: &Evenement,
    maintenant_ms: u64,
    etat: Option<&Etat>,
) -> Option<PartageVue> {
    match (actuel, evenement) {
        (PartageVue::Disponible { ecran, .. }, Evenement::Disponible { fenetre }) => Some(PartageVue::Disponible {
            debut_ms: maintenant_ms,
            fenetre_s: fenetre.as_secs(),
            ecran: *ecran,
        }),
        (PartageVue::Disponible { ecran, .. }, Evenement::DemandeRecue { expediteur_device_id, .. }) => {
            let spectateur = etat
                .and_then(|e| e.amis.iter().find(|a| a.appareils.iter().any(|ap| ap.id == *expediteur_device_id)))
                .map(|a| a.discord_name.clone());
            Some(PartageVue::Diffuse { spectateur, depuis_ms: maintenant_ms, debit_mbps: 0.0, rtt_ms: 0.0, ecran: *ecran })
        }
        (PartageVue::Diffuse { spectateur, ecran, .. }, Evenement::Connecte { .. }) => Some(PartageVue::Diffuse {
            spectateur: spectateur.clone(),
            depuis_ms: maintenant_ms,
            debit_mbps: 0.0,
            rtt_ms: 0.0,
            ecran: *ecran,
        }),
        (
            PartageVue::Diffuse { spectateur, depuis_ms, ecran, .. },
            Evenement::Mesures(Mesures::Envoi { debit_mbps, rtt_ms, .. }),
        ) => Some(PartageVue::Diffuse {
            spectateur: spectateur.clone(),
            depuis_ms: *depuis_ms,
            debit_mbps: *debit_mbps,
            rtt_ms: *rtt_ms,
            ecran: *ecran,
        }),
        (PartageVue::Demande { ami, .. }, Evenement::DemandeEnvoyee { .. }) => {
            Some(PartageVue::Demande { ami: ami.clone(), debut_ms: maintenant_ms })
        }
        (PartageVue::Demande { ami, .. }, Evenement::Connecte { en, .. }) => Some(PartageVue::Regarde {
            ami: ami.clone(),
            connecte_en_s: en.as_secs_f64(),
            debit_mbps: 0.0,
            images_par_s: 0,
            gigue_ms: 0.0,
            depuis_ms: maintenant_ms,
        }),
        (
            PartageVue::Regarde { ami, connecte_en_s, depuis_ms, .. },
            Evenement::Mesures(Mesures::Reception { debit_mbps, images_par_s, gigue_ms }),
        ) => Some(PartageVue::Regarde {
            ami: ami.clone(),
            connecte_en_s: *connecte_en_s,
            debit_mbps: *debit_mbps,
            images_par_s: *images_par_s,
            gigue_ms: *gigue_ms,
            depuis_ms: *depuis_ms,
        }),
        _ => None,
    }
}
```

Dans `materiel.rs`, au-dessus du module de tests :

```rust
use sky_encode::{pick_best, Codec, EncodeError, EncoderCaps};

/// Le codec de partage de cette machine, ou `None` sans carte NVIDIA — même
/// choix que `sky-probe hw` (`pick_best(&caps, true)` : netteté du texte).
///
/// `probe_hardware` ouvre `nvcuda.dll` par `cudarc` 0.16.6, qui PANIQUE quand
/// la bibliothèque est absente (au lieu de rendre une erreur) : sans
/// `catch_unwind`, une machine sans pilote NVIDIA fermerait l'application au
/// lancement. Une telle machine peut encore regarder (spec §4).
pub fn detecter_nvenc(sonde: impl FnOnce() -> Result<EncoderCaps, EncodeError> + std::panic::UnwindSafe) -> Option<Codec> {
    match std::panic::catch_unwind(sonde) {
        Ok(Ok(caps)) => pick_best(&caps, true),
        Ok(Err(_)) | Err(_) => None,
    }
}
```

```bash
cd spike && cargo test -p sky-app partage && cargo test -p sky-app materiel
```
Attendu : PASS (6 tests). Neutraliser comme écrit ; rétablir.

- [ ] **Étape 3 : écrire les tests du noyau qui échouent**

Dans `essais.rs`, ajouter :

```rust
use std::time::Duration;

use sky_encode::Codec;
use sky_partage::rendez_vous::FENETRE_HOTE;
use sky_partage::{Arret, ErreurPartage, Evenement, Fin};

use crate::partage::Partageur;

/// Un partage qui annonce sa disponibilité puis attend l'arrêt, sans réseau
/// ni carte graphique.
pub(crate) struct PartageurFactice;

impl Partageur for PartageurFactice {
    fn heberger(
        &self,
        _noyau: &Noyau,
        _codec: Codec,
        _ecran: usize,
        arret: &Arret,
        evenements: &mut dyn FnMut(Evenement),
    ) -> Result<Fin, ErreurPartage> {
        evenements(Evenement::Pret);
        evenements(Evenement::Disponible { fenetre: FENETRE_HOTE });
        while !arret.est_demande() {
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(Fin::Arrete)
    }

    fn regarder(
        &self,
        _noyau: &Noyau,
        _ami: i64,
        arret: &Arret,
        _evenements: &mut dyn FnMut(Evenement),
    ) -> Result<Fin, ErreurPartage> {
        while !arret.est_demande() {
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(Fin::Arrete)
    }
}
```
et, dans `CoquilleEspion`, le champ `pub icones: Mutex<Vec<bool>>,` ; dans `contexte`,
`Branchements { …, partageur: Box::new(PartageurFactice) }`.

Dans le module `tests` de `noyau.rs` :

```rust
    use sky_encode::Codec;

    fn attendre_la_fin(noyau: &Noyau) {
        let limite = std::time::Instant::now() + Duration::from_secs(5);
        while noyau.phase() != Phase::Inactive {
            assert!(std::time::Instant::now() < limite, "le partage ne s'est pas arrêté en 5 s");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn sans_carte_nvidia_partager_est_refuse_et_rien_ne_demarre() {
        // Spec §4. Neutralisation : `d.nvenc.unwrap_or(Codec::Hevc444)` —
        // le partage démarre, l'icône change.
        let c = contexte("sky-test-app-sans-nvidia", true);
        assert_eq!(c.noyau.partager(0), Err(MESSAGE_SANS_NVIDIA.to_string()));
        assert_eq!(c.noyau.phase(), Phase::Inactive);
        assert!(c.coquille.icones.lock().unwrap().is_empty());
    }

    #[test]
    fn l_icone_change_pendant_tout_le_partage_et_revient_a_l_arret() {
        // Spec §4 : « on ne partage jamais sans le savoir ». Neutralisations,
        // chacune seule : retirer `icone_partage(true)` de `partager` ; retirer
        // `icone_partage(false)` de `terminer`.
        let c = contexte("sky-test-app-icone", true);
        c.noyau.definir_nvenc(Some(Codec::Hevc444));
        c.noyau.partager(0).unwrap();
        assert_eq!(*c.coquille.icones.lock().unwrap(), vec![true]);
        assert_eq!(c.noyau.phase(), Phase::Attente);

        // Pendant l'attente : ni second partage, ni (re)connexion (spec §6).
        assert_eq!(c.noyau.partager(0), Err(MESSAGE_PENDANT_PARTAGE.to_string()));
        assert_eq!(c.noyau.connexion(), Err(MESSAGE_PENDANT_PARTAGE.to_string()));
        assert_eq!(*c.connexions.lock().unwrap(), 0);

        c.noyau.arreter();
        attendre_la_fin(&c.noyau);
        assert_eq!(*c.coquille.icones.lock().unwrap(), vec![true, false]);
        assert_eq!(c.noyau.instantane().partage, PartageVue::Termine { fin: FinVue::Arrete });
    }

    #[test]
    fn regarder_un_ami_inconnu_est_refuse_sans_rien_demarrer() {
        let c = contexte("sky-test-app-regarder-inconnu", true);
        c.noyau.synchroniser().unwrap();
        assert!(c.noyau.regarder(99).is_err());
        assert_eq!(c.noyau.phase(), Phase::Inactive);
    }
```
(ajouter `FinVue` aux `use` du module de tests : `use crate::vue::FinVue;`).

```bash
cd spike && cargo test -p sky-app
```
Attendu : ÉCHEC à la compilation (`partager`, `definir_nvenc`, `MESSAGE_SANS_NVIDIA`, `icones`…).

- [ ] **Étape 4 : implémenter dans le noyau, la coquille et les commandes**

`coquille.rs` : le trait gagne

```rust
    /// L'icône près de l'horloge change tant que dure un partage (spec §4) :
    /// on ne partage jamais sans le savoir.
    fn icone_partage(&self, actif: bool);
```
et `CoquilleTauri` l'implémente :

```rust
    fn icone_partage(&self, actif: bool) {
        if let Some(icone) = self.app.tray_by_id(crate::ID_ICONE) {
            let image = if actif {
                tauri::include_image!("icons/partage/32x32.png")
            } else {
                tauri::include_image!("icons/32x32.png")
            };
            let _ = icone.set_icon(Some(image));
            let _ = icone.set_tooltip(Some(if actif { "SkyShare — en partage" } else { "SkyShare" }));
        }
    }
```
`CoquilleEspion` l'implémente par `self.icones.lock().unwrap().push(actif);`.

`spike/crates/sky-app/icons/partage.svg` (fond accent : visible près de l'horloge) :

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024">
  <rect width="1024" height="1024" rx="224" fill="#C4664A"/>
  <circle cx="512" cy="512" r="300" fill="none" stroke="#16120F" stroke-width="96"/>
  <circle cx="512" cy="512" r="96" fill="#F2ECE6"/>
</svg>
```

```bash
cd spike/crates/sky-app && ../../../app/node_modules/.bin/tauri icon icons/partage.svg -p 32 -o icons/partage && ls icons/partage
```
Attendu : `32x32.png`. Si le fichier porte un autre nom, utiliser celui produit dans
`include_image!` et le signaler dans le rapport.

`noyau.rs` :
- `use` : `use std::sync::Arc;`, `use sky_encode::Codec;`,
  `use sky_partage::rendez_vous::FENETRE_HOTE;`, `use sky_partage::{Arret, ErreurPartage, Evenement, Fin};`,
  `use crate::partage::{appliquer, fin_vue, maintenant_ms, Partageur};`, et `FinVue` dans le `use crate::vue::…`.
- `Branchements` gagne `pub partageur: Box<dyn Partageur>,`.
- Constante : `pub const MESSAGE_SANS_NVIDIA: &str = "Partage impossible : aucune carte NVIDIA utilisable sur cette machine.";`
- `Donnees::nvenc` devient `nvenc: Option<Codec>` (initialisé à `None`) ; ajouter `arret: Option<Arret>`
  (initialisé à `None`) ; dans `instantane`, `nvenc: d.nvenc.is_some(),`.
- Méthodes, dans `impl Noyau` :

```rust
    pub fn definir_nvenc(&self, codec: Option<Codec>) {
        self.donnees().nvenc = codec;
        self.publier();
    }

    fn publier_partage_courant(&self) {
        let partage = self.donnees().partage.clone();
        self.branchements.coquille.publier_partage(&partage);
        self.publier();
        self.reveil.sonner();
    }

    /// « Partager mon écran » (spec D3) : disponible `FENETRE_HOTE`, une
    /// demande honorée au plus. La réservation (vérification ET passage en
    /// attente) se fait sous un seul verrou : deux clics ne lancent jamais
    /// deux partages.
    pub fn partager(self: &Arc<Self>, ecran: usize) -> Result<(), String> {
        self.exiger_connexion()?;
        let arret = Arret::nouveau();
        let codec = {
            let mut d = self.donnees();
            permis_hors_partage(Phase::de(&d.partage))?;
            let codec = d.nvenc.ok_or_else(|| MESSAGE_SANS_NVIDIA.to_string())?;
            d.partage = PartageVue::Disponible { debut_ms: maintenant_ms(), fenetre_s: FENETRE_HOTE.as_secs(), ecran };
            d.arret = Some(arret.clone());
            codec
        };
        self.branchements.coquille.icone_partage(true);
        self.publier_partage_courant();
        let noyau = Arc::clone(self);
        let lancement = std::thread::Builder::new().name("partage".into()).spawn(move || {
            let issue = noyau.branchements.partageur.heberger(&noyau, codec, ecran, &arret, &mut |e| noyau.sur_evenement(&e));
            noyau.terminer(&issue);
        });
        if let Err(e) = lancement {
            self.terminer(&Err(ErreurPartage::Autre(anyhow::anyhow!("fil de partage : {e}"))));
            return Err("Le partage n'a pas pu démarrer.".to_string());
        }
        Ok(())
    }

    /// « Regarder » : `ami` est un identifiant d'UTILISATEUR.
    pub fn regarder(self: &Arc<Self>, ami: i64) -> Result<(), String> {
        self.exiger_connexion()?;
        let arret = Arret::nouveau();
        {
            let mut d = self.donnees();
            permis_hors_partage(Phase::de(&d.partage))?;
            let nom = d
                .etat
                .as_ref()
                .and_then(|e| e.amis.iter().find(|a| a.id == ami))
                .map(|a| a.discord_name.clone())
                .ok_or_else(|| MESSAGE_AMI_DISPARU.to_string())?;
            d.partage = PartageVue::Demande { ami: nom, debut_ms: maintenant_ms() };
            d.arret = Some(arret.clone());
        }
        self.publier_partage_courant();
        let noyau = Arc::clone(self);
        let lancement = std::thread::Builder::new().name("regarder".into()).spawn(move || {
            let issue = noyau.branchements.partageur.regarder(&noyau, ami, &arret, &mut |e| noyau.sur_evenement(&e));
            noyau.terminer(&issue);
        });
        if let Err(e) = lancement {
            self.terminer(&Err(ErreurPartage::Autre(anyhow::anyhow!("fil de réception : {e}"))));
            return Err("La demande n'a pas pu partir.".to_string());
        }
        Ok(())
    }

    /// « Arrêter » : lève le signal ; le fil du partage le voit en 50 ms au
    /// plus (`PAS_D_ATTENTE`), ou au tour suivant du flux.
    pub fn arreter(&self) {
        if let Some(arret) = self.donnees().arret.as_ref() {
            arret.demander();
        }
    }

    pub(crate) fn sur_evenement(&self, evenement: &Evenement) {
        let suivant = {
            let d = self.donnees();
            appliquer(&d.partage, evenement, maintenant_ms(), d.etat.as_ref())
        };
        if let Some(partage) = suivant {
            self.modifier_partage(partage);
        }
    }

    fn terminer(&self, issue: &Result<Fin, ErreurPartage>) {
        let fin = fin_vue(issue);
        {
            let mut d = self.donnees();
            d.arret = None;
            if fin == FinVue::SessionExpiree {
                d.connexion = Connexion::SessionExpiree;
            }
        }
        self.branchements.coquille.icone_partage(false);
        self.modifier_partage(PartageVue::Termine { fin });
    }
```

(`terminer` appelle `icone_partage(false)` aussi à la fin d'un `regarder` : sans effet visible, l'icône
étant déjà au repos ; l'espion l'enregistre — le test de l'icône ne passe que par `partager`.)

`commandes.rs` :

```rust
#[tauri::command]
pub async fn partager(noyau: State<'_, Arc<Noyau>>, ecran: usize) -> Result<(), String> {
    sur_un_fil(noyau.inner(), move |n| n.partager(ecran)).await?
}

#[tauri::command]
pub async fn regarder(noyau: State<'_, Arc<Noyau>>, ami: i64) -> Result<(), String> {
    sur_un_fil(noyau.inner(), move |n| n.regarder(ami)).await?
}

#[tauri::command]
pub async fn arreter(noyau: State<'_, Arc<Noyau>>) -> Result<(), String> {
    noyau.arreter();
    Ok(())
}
```

`lib.rs` :
- `generate_handler!` gagne `commandes::partager, commandes::regarder, commandes::arreter,`.
- `Branchements { …, partageur: Box::new(crate::partage::PartageurReel) }` dans `setup`.
- Dans `setup`, après la création du noyau : `noyau.definir_ecrans(lister_ecrans(app.handle()));`.
- Dans le fil de synchronisation, **avant** `noyau.demarrer();` :
  `noyau.definir_nvenc(materiel::detecter_nvenc(sky_encode::probe_hardware));`
- Nouvelle fonction :

```rust
/// Les écrans, dans l'ordre d'`EnumDisplayMonitors` — celui qu'attend
/// `WgcCapture::new` (voir le plan du jalon 1, tâche 11).
fn lister_ecrans(app: &AppHandle) -> Vec<crate::vue::EcranVue> {
    let principal = app.primary_monitor().ok().flatten().and_then(|m| m.name().cloned());
    app.available_monitors()
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(index, ecran)| {
            let taille = ecran.size();
            crate::vue::EcranVue {
                index,
                nom: format!("Écran {} — {}×{}", index + 1, taille.width, taille.height),
                principal: ecran.name().is_some() && ecran.name().cloned() == principal,
            }
        })
        .collect()
}
```

- [ ] **Étape 5 : lancer, neutraliser**

```bash
cd spike && cargo test && cargo clippy --all-targets -- -D warnings
```
Attendu : PASS partout. Appliquer chaque neutralisation écrite (une à la fois) ; rétablir.

- [ ] **Étape 6 : commiter et pousser**

```bash
git add spike/crates/sky-app/Cargo.toml
git add spike/Cargo.lock
git add spike/crates/sky-app/src/partage.rs
git add spike/crates/sky-app/src/materiel.rs
git add spike/crates/sky-app/src/noyau.rs
git add spike/crates/sky-app/src/coquille.rs
git add spike/crates/sky-app/src/essais.rs
git add spike/crates/sky-app/src/commandes.rs
git add spike/crates/sky-app/src/lib.rs
git add spike/crates/sky-app/icons/partage.svg
git add spike/crates/sky-app/icons/partage/32x32.png
git diff --cached --stat
git commit -m "feat: partager, regarder, arreter dans l application — icone de partage, NVENC detecte sans paniquer"
git push
```

---

## Task 12 : Interface — panneau de partage et échecs

**Ce que la tâche livre :** en bas de la barre, le choix de l'écran (principal par défaut) et
« Partager mon écran » — désactivé sans carte NVIDIA, avec la raison écrite ; pendant un partage, la
zone passe en orange : « En partage », l'écran, le temps restant, Arrêter. Au centre, le panneau :
côté hôte, qui regarde, depuis quand, le débit envoyé et l'aller-retour ; côté spectateur,
l'attente (60 s au plus), puis « Connecté en X s · connexion directe, sans relais », le débit reçu,
les images/s, la gigue, la durée, Arrêter — et la phrase qui dit que l'image n'est pas encore
affichée (spec D2) ; à la fin, la cause **en clair** (spec §4).

**Files:**
- Create: `app/src/messages.ts`, `app/src/messages.test.ts`, `app/src/composants/useMaintenant.ts`, `app/src/composants/BarrePartage.tsx`, `app/src/composants/PanneauPartage.tsx`, `app/src/composants/Partage.test.tsx`
- Modify: `app/src/pont.ts`, `app/src/App.tsx`

**Interfaces:**
- Consumes : `PartageVue`, `FinVue`, `EcranVue` (`types.ts`, T8) ; commandes `partager`, `arreter` (T11).
- Produces : `messageDeFin(fin: FinVue): string` ; `duree(ms: number): string` ; `useMaintenant(periodeMs?: number): number` ; `pont.partager(ecran: number)`, `pont.arreter()` ; composants `BarrePartage({ instantane })`, `PanneauPartage({ instantane })`.

- [ ] **Étape 1 : écrire les tests qui échouent**

`app/src/messages.test.ts` :

```ts
import { describe, expect, it } from "vitest";
import { duree, messageDeFin } from "./messages";

describe("messages de fin", () => {
  it("les quatre échecs de la spec, mot pour mot", () => {
    // Spec §4, « Échecs, tous en clair ». Un texte changé ici change une
    // promesse faite à l'utilisateur : le test rougit.
    expect(messageDeFin({ cause: "pas_en_partage", ami: "Bob" })).toBe("Bob n'est pas en partage");
    expect(messageDeFin({ cause: "reseau_bloque" })).toBe(
      "Aucune connexion directe n'a pu s'établir entre vos deux réseaux",
    );
    expect(messageDeFin({ cause: "trop_lente" })).toBe("La connexion était trop lente pour la vidéo");
    expect(messageDeFin({ cause: "session_expiree" })).toBe("Session expirée — reconnecte-toi");
  });

  it("les autres fins, et les durées", () => {
    expect(messageDeFin({ cause: "autre", message: "Connexion interrompue : x" })).toBe("Connexion interrompue : x");
    expect(messageDeFin({ cause: "arrete" })).toBe("Partage arrêté.");
    expect(duree(125_000)).toBe("2 min 05 s");
    expect(duree(-5)).toBe("0 min 00 s");
  });
});
```

`app/src/composants/Partage.test.tsx` :

```tsx
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { pont } from "../pont";
import { instantaneDeTest } from "../test/fabriques";
import { BarrePartage } from "./BarrePartage";
import { PanneauPartage } from "./PanneauPartage";

vi.mock("../pont", () => ({ pont: { partager: vi.fn(), arreter: vi.fn() } }));

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(pont.partager).mockResolvedValue(undefined);
  vi.mocked(pont.arreter).mockResolvedValue(undefined);
});

const DEUX_ECRANS = [
  { index: 0, nom: "Écran 1 — 1920×1080", principal: false },
  { index: 1, nom: "Écran 2 — 2560×1440", principal: true },
];

describe("barre de partage", () => {
  it("partage l'écran principal par défaut", async () => {
    // Spec §4. Neutralisation : initialiser le choix à 0.
    render(<BarrePartage instantane={instantaneDeTest({ ecrans: DEUX_ECRANS })} />);
    await userEvent.click(screen.getByRole("button", { name: "Partager mon écran" }));
    expect(pont.partager).toHaveBeenCalledWith(1);
  });

  it("sans carte NVIDIA, Partager est désactivé et le dit", () => {
    // Neutralisation : retirer `!instantane.nvenc ||` du `disabled`.
    render(<BarrePartage instantane={instantaneDeTest({ nvenc: false })} />);
    expect(screen.getByRole("button", { name: "Partager mon écran" })).toBeDisabled();
    expect(screen.getByText(/aucune carte NVIDIA/)).toBeInTheDocument();
  });

  it("en partage, la zone dit En partage et propose Arrêter", async () => {
    // Spec §4 et §8, état « en partage ». Neutralisation : ne pas traiter
    // `disponible` comme un partage — le bouton Partager réapparaît.
    render(
      <BarrePartage
        instantane={instantaneDeTest({
          ecrans: DEUX_ECRANS,
          partage: { etat: "disponible", debutMs: Date.now(), fenetreS: 1800, ecran: 1 },
        })}
      />,
    );
    const zone = within(screen.getByRole("region", { name: "Partage en cours" }));
    expect(zone.getByText("En partage")).toBeInTheDocument();
    expect(zone.getByText("Écran 2 — 2560×1440")).toBeInTheDocument();
    expect(zone.getByText(/Temps restant/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Partager mon écran" })).toBeNull();
    await userEvent.click(zone.getByRole("button", { name: "Arrêter" }));
    expect(pont.arreter).toHaveBeenCalledTimes(1);
  });
});

describe("panneau de partage", () => {
  it("le spectateur voit la connexion directe, et que l'image n'est pas encore là", () => {
    // Spec D2 : « Le panneau le dit en toutes lettres ».
    render(
      <PanneauPartage
        instantane={instantaneDeTest({
          partage: { etat: "regarde", ami: "Bob", connecteEnS: 0.6, debitMbps: 12.4, imagesParS: 107, gigueMs: 5, depuisMs: Date.now() },
        })}
      />,
    );
    expect(screen.getByText("Connecté en 0.6 s · connexion directe, sans relais")).toBeInTheDocument();
    expect(screen.getByText(/image n'est pas encore affichée/)).toBeInTheDocument();
    expect(screen.getByText("107 images/s")).toBeInTheDocument();
  });

  it("une fin affiche sa cause en clair", () => {
    render(<PanneauPartage instantane={instantaneDeTest({ partage: { etat: "termine", fin: { cause: "pas_en_partage", ami: "Bob" } } })} />);
    expect(screen.getByRole("status")).toHaveTextContent("Bob n'est pas en partage");
  });
});
```

```bash
npm --prefix app test
```
Attendu : FAIL — modules introuvables.

- [ ] **Étape 2 : écrire les modules**

`app/src/messages.ts` :

```ts
import type { FinVue } from "./types";

/** Spec §4 : chaque fin, en clair. Les quatre premiers textes sont ceux de la spec. */
export function messageDeFin(fin: FinVue): string {
  switch (fin.cause) {
    case "pas_en_partage":
      return `${fin.ami} n'est pas en partage`;
    case "reseau_bloque":
      return "Aucune connexion directe n'a pu s'établir entre vos deux réseaux";
    case "trop_lente":
      return "La connexion était trop lente pour la vidéo";
    case "session_expiree":
      return "Session expirée — reconnecte-toi";
    case "arrete":
      return "Partage arrêté.";
    case "aucune_demande":
      return "Personne n'a demandé à regarder pendant 30 minutes.";
    case "autre":
      return fin.message;
  }
}

/** « 2 min 05 s » ; une durée négative vaut zéro. */
export function duree(ms: number): string {
  const secondes = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(secondes / 60)} min ${String(secondes % 60).padStart(2, "0")} s`;
}
```

`app/src/composants/useMaintenant.ts` :

```ts
import { useEffect, useState } from "react";

/** L'heure, rafraîchie toutes les `periodeMs` : temps restant, durées. */
export function useMaintenant(periodeMs = 1000): number {
  const [maintenant, setMaintenant] = useState(() => Date.now());
  useEffect(() => {
    const minuterie = setInterval(() => setMaintenant(Date.now()), periodeMs);
    return () => clearInterval(minuterie);
  }, [periodeMs]);
  return maintenant;
}
```

`app/src/composants/BarrePartage.tsx` :

```tsx
import { useState } from "react";
import { duree } from "../messages";
import { pont } from "../pont";
import type { Instantane } from "../types";
import { useMaintenant } from "./useMaintenant";

/** Bas de la barre latérale, toujours visible (spec D6). */
export function BarrePartage({ instantane }: { instantane: Instantane }) {
  const principal = instantane.ecrans.find((e) => e.principal)?.index ?? 0;
  const [ecran, setEcran] = useState(principal);
  const [erreur, setErreur] = useState<string | null>(null);
  const maintenant = useMaintenant();
  const partage = instantane.partage;

  if (partage.etat === "disponible" || partage.etat === "diffuse") {
    const nomEcran = instantane.ecrans.find((e) => e.index === partage.ecran)?.nom ?? `Écran ${partage.ecran + 1}`;
    return (
      <div role="region" aria-label="Partage en cours" className="flex flex-col gap-2 rounded-md bg-accent p-3 text-fond">
        <strong>En partage</strong>
        <span>{nomEcran}</span>
        {partage.etat === "disponible" && (
          <span>Temps restant : {duree(partage.debutMs + partage.fenetreS * 1000 - maintenant)}</span>
        )}
        <button type="button" className="rounded-md bg-fond px-3 py-1 text-texte" onClick={() => void pont.arreter()}>
          Arrêter
        </button>
      </div>
    );
  }

  const occupe = partage.etat === "demande" || partage.etat === "regarde";
  return (
    <div className="flex flex-col gap-2">
      {instantane.ecrans.length > 1 && (
        <label className="flex flex-col gap-1 text-texte-2">
          Écran
          <select value={ecran} onChange={(e) => setEcran(Number(e.target.value))} className="rounded-md bg-surface-haute px-2 py-1">
            {instantane.ecrans.map((e) => (
              <option key={e.index} value={e.index}>
                {e.nom}
                {e.principal ? " (principal)" : ""}
              </option>
            ))}
          </select>
        </label>
      )}
      <button
        type="button"
        disabled={!instantane.nvenc || occupe}
        className="w-full rounded-md bg-accent px-3 py-2 text-fond disabled:opacity-50"
        onClick={() => {
          setErreur(null);
          pont.partager(ecran).catch((e: unknown) => setErreur(String(e)));
        }}
      >
        Partager mon écran
      </button>
      {!instantane.nvenc && (
        <p className="text-xs text-texte-3">Partage impossible : aucune carte NVIDIA sur cette machine. Tu peux regarder.</p>
      )}
      {erreur && (
        <p role="alert" className="text-xs text-alerte">
          {erreur}
        </p>
      )}
    </div>
  );
}
```

`app/src/composants/PanneauPartage.tsx` :

```tsx
import { duree, messageDeFin } from "../messages";
import { pont } from "../pont";
import type { Instantane } from "../types";
import { useMaintenant } from "./useMaintenant";

function Mesure({ libelle, valeur }: { libelle: string; valeur: string }) {
  return (
    <div className="flex flex-col">
      <dt className="text-texte-3">{libelle}</dt>
      <dd>{valeur}</dd>
    </div>
  );
}

function BoutonArreter() {
  return (
    <button type="button" className="self-start rounded-md bg-surface-haute px-4 py-2" onClick={() => void pont.arreter()}>
      Arrêter
    </button>
  );
}

/** Le panneau central du partage (spec §4). */
export function PanneauPartage({ instantane }: { instantane: Instantane }) {
  const maintenant = useMaintenant();
  const partage = instantane.partage;
  const cadre = "mb-6 flex flex-col gap-3 rounded-md bg-surface p-4";

  switch (partage.etat) {
    case "inactif":
      return null;
    case "disponible":
      return (
        <section aria-label="Partage" className={cadre}>
          <h2 className="font-titre text-2xl">Tu es disponible</h2>
          <p className="text-texte-2">Tes amis peuvent te demander à regarder. Personne ne regarde encore.</p>
        </section>
      );
    case "diffuse":
      return (
        <section aria-label="Partage" className={cadre}>
          <h2 className="font-titre text-2xl">{partage.spectateur ?? "Un ami"} regarde ton écran</h2>
          <dl className="flex gap-8">
            <Mesure libelle="Depuis" valeur={duree(maintenant - partage.depuisMs)} />
            <Mesure libelle="Débit envoyé" valeur={`${partage.debitMbps.toFixed(1)} Mbps`} />
            <Mesure libelle="Aller-retour" valeur={`${Math.round(partage.rttMs)} ms`} />
          </dl>
        </section>
      );
    case "demande":
      return (
        <section aria-label="Partage" className={cadre}>
          <h2 className="font-titre text-2xl">Demande envoyée à {partage.ami}</h2>
          <p className="text-texte-2">J'attends sa réponse, 60 s au plus…</p>
          <BoutonArreter />
        </section>
      );
    case "regarde":
      return (
        <section aria-label="Partage" className={cadre}>
          <h2 className="font-titre text-2xl">Connecté en {partage.connecteEnS.toFixed(1)} s · connexion directe, sans relais</h2>
          <p className="text-texte-2">
            L'image n'est pas encore affichée : elle arrive au jalon 2. SkyShare mesure la connexion, puis jette la vidéo
            reçue — rien n'est écrit sur le disque.
          </p>
          <dl className="flex gap-8">
            <Mesure libelle="Débit reçu" valeur={`${partage.debitMbps.toFixed(1)} Mbps`} />
            <Mesure libelle="Cadence" valeur={`${partage.imagesParS} images/s`} />
            <Mesure libelle="Gigue" valeur={`${partage.gigueMs.toFixed(1)} ms`} />
            <Mesure libelle="Durée" valeur={duree(maintenant - partage.depuisMs)} />
          </dl>
          <BoutonArreter />
        </section>
      );
    case "termine":
      return (
        <section aria-label="Partage" className={cadre}>
          <p role="status">{messageDeFin(partage.fin)}</p>
        </section>
      );
  }
}
```

Dans `app/src/pont.ts`, ajouter à l'objet `pont` :

```ts
  partager: (ecran: number) => invoke<void>("partager", { ecran }),
  arreter: () => invoke<void>("arreter"),
```

Dans `app/src/App.tsx` : importer `BarrePartage` et `PanneauPartage` ; remplacer la prop `bas`
par `bas={<BarrePartage instantane={instantane} />}` ; ajouter `<PanneauPartage instantane={instantane} />`
comme **premier** enfant de `Disposition`, avant les écrans.

- [ ] **Étape 3 : lancer, neutraliser, construire**

```bash
npm --prefix app test && npm --prefix app run build
```
Attendu : 21 tests verts. Neutraliser chaque test selon son commentaire ; rétablir.

- [ ] **Étape 4 : commiter et pousser**

```bash
git add app/src/messages.ts
git add app/src/messages.test.ts
git add app/src/composants/useMaintenant.ts
git add app/src/composants/BarrePartage.tsx
git add app/src/composants/PanneauPartage.tsx
git add app/src/composants/Partage.test.tsx
git add app/src/pont.ts
git add app/src/App.tsx
git diff --cached --stat
git commit -m "feat: barre et panneau de partage, echecs en clair"
git push
```

---

## Task 13 : Essai réel final et installateur (propriétaire)

**Ce qui clôt le jalon (spec §8).** Même protocole que le C2 : deux machines, **deux réseaux**,
deux comptes Discord. L'implémenteur ne peut pas le faire.

**Files:**
- Modify: `tasks/todo.md` (résultats)

- [ ] **Étape 1 (contrôleur) : revue au périmètre de la branche entière**

Avant l'essai : une relecture de **tout** le diff `main..jalon-1-application` d'un seul tenant
(leçon du 23/08 : les défauts nés entre deux tâches ne sont vus par aucune revue de tâche). Points à
chercher en particulier : un texte qui promet un comportement changé depuis (panneau, Mon compte,
messages) ; une commande Tauri déclarée mais absente de `generate_handler!` ; un appel de
`sky_compte::synchroniser` hors de `Noyau::synchroniser` dans `sky-app` (`grep -rn "synchroniser(" spike/crates/sky-app/src`
ne doit montrer que le `Noyau` et `PartageurReel`) ; un `println!` resté dans `sky-partage`
(`grep -rn "println" spike/crates/sky-partage/src` : aucune ligne).

- [ ] **Étape 2 (contrôleur) : l'installateur final**

```bash
cd spike && cargo test && cargo clippy --all-targets -- -D warnings
npm --prefix app test && npm --prefix app run build
cd spike/crates/sky-app && ../../../app/node_modules/.bin/tauri build
```
Relever chemin et taille de `SkyShare_0.1.0_x64-setup.exe`.

- [ ] **Étape 3 (propriétaire) : le mode d'emploi**

1. Sur les deux machines : installer la nouvelle version par-dessus l'ancienne (même procédure
   qu'en T9 ; si l'installateur propose de désinstaller la précédente, accepter). Réautoriser dans
   Avira si demandé.
2. Mettre les deux machines sur **deux réseaux différents** (par exemple le portable sur le partage
   de connexion d'un téléphone), comme au C2.
3. Machine A : **Partager mon écran** (l'écran principal est proposé). Vérifier : la zone du bas
   passe en orange « En partage » avec le temps restant ; **l'icône près de l'horloge change**.
4. Machine B : Amis → **Regarder** A. Chronométrer du clic à « Connecté en X s » ; noter X affiché.
   Laisser tourner 2 minutes. Relever, à 1 min et à 2 min : côté B débit reçu, images/s, gigue ;
   côté A qui regarde, débit envoyé, aller-retour.
5. B : **Arrêter**. A : le panneau affiche une fin (noter le texte). A : **Arrêter** ; l'icône
   revient au repos.
6. Échec attendu : A **n'est pas** en partage ; B clique Regarder. Au bout de 60 s, B doit lire
   « A n'est pas en partage » (avec le nom Discord de A). Noter le texte exact.
7. Machine sans carte NVIDIA, si l'une des deux l'est : Partager doit être désactivé avec la
   raison, sans que l'application se ferme au lancement.
8. Veille (spec §10, question ouverte) : A clique Partager puis ne touche plus à rien pendant 30
   minutes. Noter si Windows met la machine en veille, et ce qu'affiche SkyShare au réveil. **Rien
   n'est décidé ici** : on observe.

- [ ] **Étape 4 (contrôleur) : consigner et clore**

Dans `tasks/todo.md`, section « Jalon 1 — essai réel final » : chaque point 3 à 8, réussi ou non, les
délais et mesures relevés, les textes affichés. Si tout est vert, le jalon est clos : proposer la
suite par `superpowers:finishing-a-development-branch` (fusion de `jalon-1-application` dans
`main`, sur accord). Tout échec : diagnostic avant toute fusion.

```bash
git add tasks/todo.md
git commit -m "docs: jalon 1 — essai reel final"
git push
```

---

## Auto-relecture du plan

**Couverture de la spec, section par section.**

| Spec | Tâche(s) |
|---|---|
| §1 Où l'on part | contexte ; T4-T5 reprennent `rendez_vous.rs` et la colle C2 |
| §2 D1 (quatre écrans + partage) | T8 (Connexion, Amis), T10 (Listes, Mon compte), T11-T12 (partage) |
| §2 D2 (la connexion, pas l'image ; flux jeté) | T5 (`regarder` sans puits), T11 (`|| Ok(None)`), T12 (phrase du panneau, testée) |
| §2 D3 (disponibilité explicite, 30 min / 60 s / 2 s) | T5 (`FENETRE_HOTE`, `ATTENTE_SPECTATEUR`, `CADENCE` inchangées), T7 (cadence 2 s en attente), T11 (partage seulement sur clic) |
| §2 D4 (icône, fermer = réduire, démarrage, instance unique) | T6 (test d'instance unique sur le vrai exécutable), T7 (visibilité), T10 (case Mon compte) |
| §2 D5 (bibliothèque commune) | T4, T5 |
| §2 D6 (barre latérale, DA) | T6 (`Disposition`, jetons relevés dans `globals.css`), T12 (bouton de partage en bas) |
| §3 Architecture (crates, boucle unique, commandes, événements `etat`/`partage`, fils dédiés) | T4-T7, T11 ; `commandes.rs` passe par `spawn_blocking` |
| §4 Écrans et parcours, échecs en clair | T8, T10, T12 ; les quatre messages testés mot pour mot (T12) et leurs causes (T11) |
| §5 Changement côté site | T1 ; mise en production en T9 étape 1, sur accord |
| §6 Limites (sky-probe vole, pas de reconnexion pendant un partage, écart 7 affiché, non signée) | T10 (phrase testée), T7/T11 (tests), T11/T12 (`TropLente`), T9 (SmartScreen) |
| §7 Hors périmètre | rien n'est planifié qui y figure |
| §8 Vérification | T4 (tests déplacés, comptés, `diff`), T2/T3 (double dérivé du site, chaque paramètre contre sa ligne), T6 (instance unique), T7 (horloge injectée), T8/T10/T12 (composants qui décident), T9 et T13 (essais réels) |
| §9 Installation (NSIS) | T6 étape 9, T9, T13 |
| §10 Questions ouvertes | veille : T13 étape 3.8 (observation) ; nom par défaut : T7 (`nom_d_appareil`, testé) |

**Points de la spec non couverts, ou tranchés ici :**
1. **Renommer l'appareil dans Mon compte** (spec §4) : **non couvert** — aucune route du site ne
   le permet, et la spec §5 n'autorise qu'une modification du site. Reporté, signalé.
2. **Message réseau — arbitré par le contrôleur :** le texte devient « Aucune connexion directe n'a
   pu s'établir entre vos deux réseaux » (un constat, pas une cause), spec §4 corrigée. Note
   d'origine : « Ton réseau bloque la connexion directe » n'était affiché que si ICE n'a trouvé aucun chemin
   (T11, `fin_vue`) ; sinon un message qui n'accuse pas le réseau. Même ce cas reste une
   conjecture (les leçons du 23/08 : un diagnostic n'énonce que ce qu'il a mesuré) : le texte vient
   de la spec, le propriétaire peut vouloir l'adoucir.
3. **Démarrage automatique activé au premier lancement, version publiée seulement** (T6).
4. **`sky-probe view` écrit toujours `recu.h265` par défaut** : « exactement comme à l'essai réel »
   l'emporte ; l'option est dans l'API de `sky-partage` (T5).
5. **Défaut du site relevé, hors périmètre :** retirer un ami ou supprimer une liste peut ne pas
   faire progresser la version ; l'application le contourne pour ses propres gestes
   (resynchronisation complète après chaque commande, T7), pas pour ceux de l'autre partie (T3).
6. **Tâche « extraction » coupée en deux** (T4/T5) : treize tâches au lieu de douze.

**Recherche de marqueurs.** Aucun « TBD », « TODO », « à compléter », « similaire à la tâche ».
Deux blocs de T5 (`hote.rs`) et un de T5 (`cmd_host.rs` : bilan, diagnostic) sont des
**déplacements** désignés par plages de lignes du fichier d'origine, avec la table exacte des
substitutions : ce n'est pas du code à inventer, c'est du code existant à recopier sans le
réécrire — la règle même de la spec (§8, « déplacés, pas réécrits »).

**Cohérence des types entre tâches — défauts trouvés et corrigés pendant la relecture :**
1. **`Ami` sans `friendship_id`** : les routes `friends/{id}` lisent un identifiant d'amitié, que
   `Ami` ne portait pas (seule `Demande` l'avait). Sans lui, Retirer et Bloquer étaient
   inimplémentables. Ajouté en T3, avec ses sept littéraux de test, dont six dans
   `rendez_vous.rs` — arbitré dans la table de propriété (avant le déplacement de T4).
2. **`Etat::listes`** : même piège — sept littéraux `Etat { … }` dans quatre fichiers (dont un
   `EtatBrut` de test dans `annuaire.rs:751`) cassaient la compilation. Tous listés en T2.
3. **`cmd_encode::FPS`** était partagé avec `cmd_codecs.rs` : déplacer la constante aurait cassé
   `sky-probe codecs`. Elle reprend `sky_partage::FPS` au lieu de disparaître (T5).
4. **`Branchements` gagne `partageur` en T11** : tous les tests construisent le noyau par
   `essais::contexte`, seul endroit à modifier — pas de réécriture des tests de T7-T10.
5. **`Donnees::nvenc`** passe de `bool` (T7) à `Option<Codec>` (T11) : écrit explicitement en T11,
   avec la ligne de `instantane` qui en dépend.
6. **`message_erreur`** : le `Display` d'`ErreurCompte::Protocole` préfixe « réponse inattendue du
   serveur », faux pour une entrée refusée avant le réseau ; seul le détail est montré (T7).
7. **Une référence en avant assumée, et dite** : le bouton Regarder (T8) invoque `regarder`,
   commande de T11. Rien ne compile contre elle en Rust ; l'interface affiche l'erreur de Tauri
   entre-temps, et T9 demande de ne pas cliquer.
8. **Numérotation** : la liste du contrôleur est décalée d'un cran à partir de T5 ; la table de
   découpage donne la correspondance.

**Signatures du code qui contredisaient la spec ou le brief :**
- `sky_compte::Ami` n'a pas `friendshipId` (voir 1 ci-dessus).
- `revoquer_appareil` existait déjà mais était **privée** (`annuaire.rs:475`) ; `nom_appareil_valide`
  aussi (`annuaire.rs:341`).
- Le repli existant de `sky-compte` pour un nom d'appareil refusé est « Appareil SkyShare »
  (`nom_par_defaut`, `annuaire.rs:489-494`), pas « PC » comme le voulait la spec §10.
  **Arbitrage du contrôleur :** l'application prend le **même** repli, « Appareil SkyShare » (T7) —
  un seul nom générique dans le produit ; la spec §10 est corrigée en conséquence.
- `probe_hardware` ne rend pas d'erreur sans pilote NVIDIA : `cudarc` panique (T11).

