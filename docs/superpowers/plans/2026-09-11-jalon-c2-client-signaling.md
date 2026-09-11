# Jalon C2 — le client de signaling — plan d'implémentation

> **Pour les agents :** SOUS-COMPÉTENCE REQUISE — utiliser
> `superpowers:subagent-driven-development` pour exécuter ce plan tâche par tâche.
> Les étapes utilisent des cases à cocher (`- [ ]`).

**But :** deux machines établissent une connexion pair-à-pair sans aucun copier-coller.

**Architecture :** un crate nouveau `sky-compte` porte toute la logique (session, coffre,
annuaire, boîte) et ne sait rien du terminal ; `sky-probe` l'appelle et affiche. `sky-net`
voit le sens de l'échange s'inverser — c'est le spectateur qui produit l'offre. Le site
apprend à faire passer un compte protégé par son second facteur pendant le flux natif.

**Pile :** Rust 1.94, édition 2021, workspace `spike/`. `ureq` (HTTP), `tiny_http`
(boucle locale), `keyring` (coffre-fort système), `serde`/`serde_json`, `str0m` 0.23.
Côté site : Next.js 16.2.2, Vitest.

**Spec :** `docs/superpowers/specs/2026-09-11-jalon-c2-client-signaling-design.md`

---

## Contraintes globales

Elles s'appliquent à **toutes** les tâches, sans être répétées dans chacune.

- **Le français, accents compris**, dans le code, les commentaires et les commits.
  Identifiants du domaine en français (`deposer`, `relever`, `Coffre`, `Annuaire`).
- **Aucun test automatisé ne touche la base de production.** Elle contient 11 comptes
  Discord réels et 5 sessions réelles. Les tests tournent contre le serveur double.
- **Ne jamais imprimer une chaîne de connexion ni un jeton**, même partiellement, même
  tronqué — ni dans un message d'erreur, ni dans une trace.
- **Ne jamais utiliser `RUST_LOG="str0m=debug"`.** Ce qui garantit « aucune adresse
  journalisée », c'est qu'aucun collecteur `tracing` n'est installé. Ne pas en installer.
- **Pas de runtime asynchrone.** Pas de `tokio`, pas d'`async fn`.
- **Ne jamais écrire un fichier contenant des antislashs, du JSON ou une expression
  régulière via un heredoc Bash ou `node -e`** — un antislash y a été mangé trois fois.
  Utiliser les outils d'édition.
- **`cargo test` et `cargo clippy -- -D warnings` doivent passer** à la fin de chaque tâche.
- **Ne pas pousser sur `main` du dépôt du site** sans accord explicite du propriétaire :
  le distant déclenche un déploiement en production.
- **Types de l'API, à ne pas confondre :** `Enveloppe.id` est une **chaîne** (BIGSERIAL) ;
  `expediteur_device_id` et `destinataire_device_id` sont des **entiers** (INTEGER).

---

## Propriété des fichiers

Un fichier a **un seul** propriétaire. Toute autre tâche qui a besoin d'y toucher doit
passer par l'interface que le propriétaire expose. Cette table existe parce qu'au jalon A,
des fichiers sans propriétaire déclaré n'ont été relus par personne.

| Fichier | Tâche propriétaire |
|---|---|
| `spike/crates/sky-net/src/link.rs` | T1 |
| `spike/crates/sky-probe/src/cmd_selftest.rs` | T1 |
| `spike/crates/sky-crypto/src/lib.rs` | T5 |
| `spike/crates/sky-compte/Cargo.toml` | T3 |
| `spike/crates/sky-compte/src/erreur.rs` | T3 |
| `spike/crates/sky-compte/src/http.rs` | T3 |
| `spike/crates/sky-compte/src/lib.rs` | T3 |
| `spike/crates/sky-compte/tests/faux_serveur/mod.rs` | T4 |
| `spike/crates/sky-compte/src/coffre.rs` | T5 |
| `spike/crates/sky-compte/src/session.rs` | T6 (T7 le prolonge) |
| `spike/crates/sky-compte/src/annuaire.rs` | T8 |
| `spike/crates/sky-compte/src/boite.rs` | T9 |
| `spike/crates/sky-probe/src/cmd_compte.rs` | T10 |
| `spike/crates/sky-probe/src/main.rs` | T10 |
| `spike/crates/sky-probe/src/cmd_host.rs` | T11 |
| `spike/crates/sky-probe/src/cmd_view.rs` | T11 |
| `spike/Cargo.toml` (dépendances du workspace) | T3 |
| Site : `src/app/api/auth/callback/route.ts` | T2 |
| Site : `src/app/api/auth/totp/verify/route.ts` | T2 |
| Site : `src/app/[locale]/(auth)/totp/page.tsx` | T2 |
| Site : `src/app/api/sky/**` (câblage `sontAmis`) | T2 |

---

## Task 1 : l'inversion des rôles dans `sky-net`

C'est la tâche la plus risquée du jalon, donc elle passe en premier : si elle résiste, tout
le reste attend. Elle est validable **sans réseau et sans correspondant** grâce à
`sky-probe selftest`, qui négocie entre deux liens du même processus.

**Ce qu'il faut comprendre avant de toucher au code.** Aujourd'hui `PeerLink::host()`
produit l'offre et crée le canal de données ; `PeerLink::viewer(offre)` la consomme et
produit la réponse. La spec d'architecture (décision D2, tranchée le 23/08) inverse ce
sens : **c'est le spectateur qui produit l'offre**. Un hôte qui offre publierait ses
adresses avant de savoir à qui — mesuré sur le spike, deux adresses lisibles en clair.

**Ce que la tâche fait :** elle sépare le **rôle de signaling** (qui offre) du **rôle média**
(qui envoie la vidéo), qui étaient confondus dans les noms.

**Files:**
- Modify: `spike/crates/sky-net/src/link.rs:131` (`host`) et `:193` (`viewer`)
- Modify: `spike/crates/sky-probe/src/cmd_selftest.rs`

**Interfaces:**
- Produces :
  - `PeerLink::offrant(identity: Identity) -> anyhow::Result<(Self, String)>`
  - `PeerLink::repondant(identity: Identity, offre_texte: &str) -> anyhow::Result<(Self, String)>`
  - `PeerLink::accepter_reponse(&mut self, texte: &str) -> anyhow::Result<()>`
  - Tout le reste de `PeerLink` est inchangé (`poll`, `send`, `is_connected`,
    `guetter_le_pair`, `maintenir_mapping`, `trafic`, `peer_key`, `canal_ouvert`).

- [ ] **Étape 1 : renommer sans rien changer d'autre**

`host` → `offrant`, `viewer` → `repondant`, `accept_answer` → `accepter_reponse`.
Mettre à jour les appelants : `cmd_selftest.rs`, `cmd_host.rs`, `cmd_view.rs`.

Les noms `host`/`viewer` décrivaient un rôle média ; ils deviennent faux dès que le sens
s'inverse, et un nom faux est pire qu'un nom vague. Corriger aussi le commentaire de
`offrant` : il dit « côté émetteur », ce ne sera plus vrai.

- [ ] **Étape 2 : vérifier que rien n'a bougé**

```bash
cd spike && cargo test && cargo run -p sky-probe -- selftest
```
Attendu : `selftest` négocie et ouvre le canal, comme avant. C'est un renommage pur.

- [ ] **Étape 3 : commiter le renommage seul**

```bash
git add -A && git commit -m "refactor: sky-net nomme les roles de signaling, pas les roles media"
```

Un commit séparé, pour que le diff de l'inversion qui suit ne soit pas noyé dans du bruit
de renommage.

- [ ] **Étape 4 : écrire le test qui échoue**

Dans `spike/crates/sky-net/src/link.rs`, module `tests` :

```rust
#[test]
fn le_spectateur_offre_et_l_hote_repond() {
    // Le sens exige par la spec d'architecture (D2, 23/08) : c'est celui qui
    // veut REGARDER qui produit l'offre. L'hote ne publie jamais d'adresse
    // avant d'avoir ouvert l'offre et su a qui il parle.
    let spectateur_id = Identity::generate();
    let hote_id = Identity::generate();

    let (mut spectateur, offre) = PeerLink::offrant(spectateur_id).unwrap();
    let (mut hote, reponse) = PeerLink::repondant(hote_id, &offre).unwrap();
    spectateur.accepter_reponse(&reponse).unwrap();

    // Le canal doit s'ouvrir meme si c'est desormais l'offrant qui le cree
    // et le repondant qui le recoit par Event::ChannelOpen.
    let limite = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < limite {
        let _ = spectateur.poll();
        let _ = hote.poll();
        if spectateur.canal_ouvert() && hote.canal_ouvert() {
            return;
        }
    }
    panic!("le canal ne s'est pas ouvert dans les 10 s");
}
```

- [ ] **Étape 5 : lancer le test, vérifier qu'il échoue**

```bash
cd spike && cargo test -p sky-net le_spectateur_offre
```
Attendu : ÉCHEC. Aujourd'hui c'est `offrant` qui crée le canal et qui est appelé par
l'hôte ; rien ne garantit que le sens inverse fonctionne. **Lire le message d'échec** : s'il
passe du premier coup, c'est que le test ne mesure rien — il faut alors vérifier à la main
que les rôles sont bien inversés avant d'aller plus loin.

- [ ] **Étape 6 : faire passer le test**

Le canal de données est créé par l'offrant (`link.rs:146`,
`change.add_channel_with_config`). Les deux bords apprennent son identifiant par
`Event::ChannelOpen` (`link.rs:505`), et `send` travaille depuis `self.canal` — le chemin
d'émission est donc indifférent au rôle. Il ne devrait rien y avoir à changer dans la
création du canal.

Le point à vérifier est le **perçage du NAT** : `cibles_pair` sert à ouvrir un passage
entrant, avec ce commentaire — « sans cela, un spectateur qui attend passivement garde sa
box fermée ». Après l'inversion, c'est **l'hôte** qui attend. S'assurer que le côté
répondant renseigne bien `cibles_pair` depuis le SDP de l'offre reçue, via
`cibles_depuis_sdp`.

Mettre à jour le commentaire de confidentialité de `offrant` : il annonce que « l'offre
voyage en clair » et que « au jalon 1 l'offre sera scellée ». C'est ce jalon — le dire.

- [ ] **Étape 7 : lancer les tests et `selftest`**

```bash
cd spike && cargo test && cargo run -p sky-probe -- selftest
```
`cmd_selftest.rs` doit maintenant appeler `offrant` pour le spectateur et `repondant` pour
l'hôte, et ses messages doivent nommer les bons rôles.

- [ ] **Étape 8 : prouver le test par neutralisation**

Remettre temporairement la création du canal dans `repondant` au lieu de `offrant`.
Attendu : `le_spectateur_offre_et_l_hote_repond` rougit. Rétablir.
Noter dans le rapport ce qui a rougi, et si autre chose a rougi en même temps.

- [ ] **Étape 9 : commiter**

```bash
git add -A && git commit -m "feat: le spectateur produit l offre (D2 de la spec d architecture)"
```

---

## Task 2 : côté site — le second facteur dans le flux natif

Tâche dans l'**autre dépôt** : `D:\Mods Minecraft\EriniumGroupWebsite`. Elle est placée tôt
pour que la suite se développe contre le comportement final, pas contre une version qui
refuse encore les comptes protégés.

**Attention `core.autocrlf=true` :** `git status` marque comme modifiés des fichiers dont
le contenu est identique octet pour octet. Vérifier par `git diff --ignore-cr-at-eol`.

**Files:**
- Modify: `src/app/api/auth/callback/route.ts` (branche native, vers la ligne 100)
- Modify: `src/app/api/auth/totp/verify/route.ts`
- Modify: `src/app/[locale]/(auth)/totp/page.tsx`
- Modify: les routes `src/app/api/sky/**` qui réécrivent le prédicat d'amitié
- Test: `src/app/api/auth/__tests__/callback-natif.test.ts`

**Interfaces:**
- Consumes : `signerStateNatif(port, empreinte)`, `lireStateNatif(state)`,
  `emettreCode(userId, empreinte)` — tous dans `src/lib/sky/natif.ts`, inchangés.
- Produces : rien que l'application consomme directement. Le contrat réseau ne change pas :
  l'application reçoit toujours `http://127.0.0.1:P/?code=<code>`.

- [ ] **Étape 1 : écrire le test qui échoue**

Dans `src/app/api/auth/__tests__/callback-natif.test.ts` :

```ts
it("un compte protege par TOTP traverse le second facteur puis recoit un code", async () => {
  // Avant ce jalon, la branche native refusait : ?error=totp_requis.
  const state = signerStateNatif(PORT, empreinteDeTest);
  const reponse = await callbackAvecCompteTotp(state);

  // On part vers la page TOTP, pas vers la boucle locale, et SANS code.
  const destination = new URL(reponse.headers.get("location")!);
  expect(destination.hostname).not.toBe("127.0.0.1");
  expect(destination.searchParams.get("code")).toBeNull();
  expect(destination.pathname).toContain("/totp");

  // Le state signe voyage tel quel : ni le port ni l'empreinte en clair.
  expect(destination.searchParams.get("port")).toBeNull();
  expect(destination.searchParams.get("empreinte")).toBeNull();
  expect(destination.searchParams.get("natif")).toBe(state);
});

it("emettreCode est inatteignable sans second facteur verifie", async () => {
  // Rejoue l'attaque reproduite par le relecteur du jalon C1 :
  // « SESSION PLEINE OBTENUE SANS SECOND FACTEUR ».
  const state = signerStateNatif(PORT, empreinteDeTest);
  await callbackAvecCompteTotp(state);

  // La session partielle ne doit ouvrir AUCUN code.
  const avant = await compterAuthCodes();
  await verifierTotpAvecCodeFaux(state);
  expect(await compterAuthCodes()).toBe(avant);
});
```

- [ ] **Étape 2 : lancer, vérifier l'échec**

```bash
npx vitest run src/app/api/auth/__tests__/callback-natif.test.ts
```
Attendu : ÉCHEC — la redirection va aujourd'hui vers `127.0.0.1` avec `error=totp_requis`.

- [ ] **Étape 3 : implémenter**

Dans `callback/route.ts`, branche native : quand `user.totp_enabled`, ne plus refuser.
Créer la **session partielle** (`totpVerified: false`, exactement comme le fait déjà la
branche web juste en dessous) et rediriger vers la page TOTP en lui passant le **`state`
signé tel quel**, sous le paramètre `natif`.

Dans `totp/verify/route.ts` : si la requête porte un `natif`, le revalider par
`lireStateNatif` — **jamais** reconstruire le port depuis un paramètre — puis, après
vérification réussie du code seulement, appeler `emettreCode(user.id, natif.empreinte)` et
rendre une redirection vers `http://127.0.0.1:${natif.port}/?code=${code}`.

L'ordre est la garantie : vérification du second facteur **d'abord**, `emettreCode`
**ensuite**. Un commentaire doit le dire, en nommant l'attaque que ça ferme.

- [ ] **Étape 4 : lancer les tests**

```bash
npx tsc --noEmit && npm test && npm run build
```
Les trois, aucune ne remplace les autres : le compilateur interne de `next build` ignore
les fichiers de test.

- [ ] **Étape 5 : prouver par neutralisation**

Inverser l'ordre — appeler `emettreCode` avant la vérification du code TOTP.
Attendu : `emettreCode est inatteignable sans second facteur verifie` rougit.
Rétablir, et noter dans le rapport si un autre test a rougi en même temps : deux gardes
redondantes qui répondent l'une pour l'autre ne prouvent rien.

- [ ] **Étape 6 : câbler `sontAmis` et `proprietaireDe`**

Ces deux fonctions de `src/lib/sky/` sont correctes et **n'ont aucun appelant en
production** — les routes réécrivent leur prédicat sur place. Les brancher.

Contrainte absolue : **le nombre de requêtes SQL par cas ne doit pas changer.** L'uniformité
des refus de C1 se mesure aussi au nombre d'allers-retours (mesuré une fois : 39 ms contre
67, distributions sans recouvrement). Si le câblage ajoute une requête sur un chemin, il
faut le signaler et s'arrêter plutôt que de le livrer.

- [ ] **Étape 7 : commiter (sans pousser)**

```bash
git add -A && git commit -m "feat: le flux natif traverse le second facteur"
```

**Ne pas pousser.** La poussée sur `main` déploie en production et demande l'accord du
propriétaire — le contrôleur s'en charge.

---

## Task 3 : le squelette de `sky-compte`

**Files:**
- Create: `spike/crates/sky-compte/Cargo.toml`
- Create: `spike/crates/sky-compte/src/lib.rs`
- Create: `spike/crates/sky-compte/src/erreur.rs`
- Create: `spike/crates/sky-compte/src/http.rs`
- Modify: `spike/Cargo.toml` (dépendances du workspace)

**Interfaces:**
- Produces :
  - `pub struct Config { pub base_url: String }`, avec `Config::depuis_env()` qui lit
    `SKY_API_URL` (défaut `https://eriniumgroup.vercel.app`) et
    `Config::vers(url: &str) -> Config`, que **tous** les tests des tâches suivantes
    utilisent pour viser le serveur double
  - `ErreurCompte::depuis_statut(statut: u16, corps: &str) -> ErreurCompte`
  - `pub enum ErreurCompte { Reseau(String), Refuse, Protocole(String), Coffre(String) }`
  - `pub struct ClientHttp` avec
    `get_json<T: DeserializeOwned>(&self, chemin: &str, jeton: Option<&str>) -> Result<T, ErreurCompte>`
    et `post_json<B: Serialize, T: DeserializeOwned>(&self, chemin: &str, corps: &B, jeton: Option<&str>) -> Result<T, ErreurCompte>`

- [ ] **Étape 1 : créer le crate et le déclarer**

`spike/Cargo.toml` a `members = ["crates/*"]` : le crate est donc pris automatiquement.
Ajouter aux dépendances du workspace :

```toml
ureq = { version = "2", features = ["json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

`serde`, `serde_json` et `base64` existent déjà comme dépendances de `sky-net` mais **pas**
comme dépendances du workspace — les y remonter, et faire pointer `sky-net` dessus, plutôt
que de dupliquer des versions qui divergeront.

- [ ] **Étape 2 : écrire le test qui échoue**

```rust
#[test]
fn un_401_devient_refuse_sans_detail() {
    // Le serveur repond « Code refuse » pour quatre causes differentes,
    // volontairement indistinctes. Le client ne doit pas reintroduire la
    // distinction que le serveur a refuse de faire.
    let erreur = ErreurCompte::depuis_statut(401, "{\"error\":\"Code refuse\"}");
    assert!(matches!(erreur, ErreurCompte::Refuse));
    assert_eq!(erreur.to_string(), "identifiants refuses");
}

#[test]
fn aucun_jeton_dans_le_message_d_erreur() {
    // Un jeton dans un message d'erreur finit dans un rapport de bug.
    let erreur = ErreurCompte::Reseau("echec vers /api/sky/sync".into());
    assert!(!erreur.to_string().contains("Bearer"));
}
```

- [ ] **Étape 3 : lancer, vérifier l'échec**

```bash
cd spike && cargo test -p sky-compte
```
Attendu : ÉCHEC à la compilation — `ErreurCompte` n'existe pas.

- [ ] **Étape 4 : implémenter**

`ClientHttp` fixe un délai d'attente de **5 secondes**, pas moins :

```rust
/// Neon se suspend apres 5 minutes d'inactivite. Le reveil a ete mesure a
/// 748,8 ms contre ~35 ms a chaud. Un delai de 500 ms transformerait un
/// reveil NORMAL en panne. Ne pas « optimiser » cette valeur.
const DELAI: Duration = Duration::from_secs(5);
```

- [ ] **Étape 5 : lancer les tests**

```bash
cd spike && cargo test -p sky-compte && cargo clippy -p sky-compte -- -D warnings
```

- [ ] **Étape 6 : commiter**

```bash
git add -A && git commit -m "feat: squelette de sky-compte — erreurs et client HTTP"
```

---

## Task 4 : le serveur double

**Files:**
- Create: `spike/crates/sky-compte/tests/faux_serveur/mod.rs`
- Modify: `spike/crates/sky-compte/Cargo.toml` (`[dev-dependencies]` : `tiny_http = "0.12"`)

**Interfaces:**
- Produces :
  - `FauxServeur::demarrer() -> FauxServeur` — écoute sur un port libre
  - `FauxServeur::url(&self) -> String`
  - `FauxServeur::etat_mut(&self) -> MutexGuard<EtatFaux>` — pour préparer les réponses
  - `struct EtatFaux { pub amis: Vec<AmiFaux>, pub enveloppes: Vec<EnveloppeFausse>, pub version: u64 }`

- [ ] **Étape 1 : relever le contrat réel, sans l'inventer**

Lire les tests du site dans `src/app/api/sky/**/__tests__/` et **en dériver** les formes.
Ne pas les déduire du code de l'application : un double qui répond ce que l'application
espère, plutôt que ce que le vrai serveur répond, donne une confiance imméritée — c'est la
classe de défaut « tests verts qui ne mesurent rien ».

Formes relevées, à respecter exactement :

```
GET /api/sky/sync        -> { version, code, amis, demandes, listes, appareils, enveloppes }
                            ou { inchange: true }
Ami        = { friendshipId, id, discord_name, discord_avatar, depuis, appareils }
AppareilDAmi = { id: number, public_key: string }
Enveloppe  = { id: string, expediteur_device_id: number,
               destinataire_device_id: number, charge: string (base64) }
POST /api/auth/native { code, secret } -> jetons | 401 { error: "Code refuse" }
```

`appareils` est un tableau **vide, jamais absent**, pour un ami sans appareil.

- [ ] **Étape 2 : écrire le test qui échoue**

```rust
#[test]
fn le_double_rend_un_tableau_vide_jamais_absent() {
    let s = FauxServeur::demarrer();
    s.etat_mut().amis.push(AmiFaux::sans_appareil("bob"));

    let recu: serde_json::Value = ureq::get(&format!("{}/api/sky/sync", s.url()))
        .call().unwrap().into_json().unwrap();

    assert_eq!(recu["amis"][0]["appareils"], serde_json::json!([]));
}

#[test]
fn le_double_refuse_sans_distinguer() {
    // Les quatre causes d'echec rendent la MEME reponse, comme le vrai.
    let s = FauxServeur::demarrer();
    for corps in [r#"{"code":"inconnu","secret":"x"}"#, r#"{"code":"","secret":""}"#] {
        let e = ureq::post(&format!("{}/api/auth/native", s.url()))
            .send_string(corps).unwrap_err();
        let reponse = match e { ureq::Error::Status(_, r) => r, _ => panic!("attendu 401") };
        assert_eq!(reponse.status(), 401);
        assert_eq!(reponse.into_string().unwrap(), r#"{"error":"Code refuse"}"#);
    }
}
```

- [ ] **Étape 3 : lancer, vérifier l'échec**

```bash
cd spike && cargo test -p sky-compte faux_serveur
```

- [ ] **Étape 4 : implémenter le double**

Un `tiny_http::Server` sur un port éphémère, dans un fil, avec un `Arc<Mutex<EtatFaux>>`.

- [ ] **Étape 5 : lancer les tests et commiter**

```bash
cd spike && cargo test -p sky-compte
git add -A && git commit -m "test: serveur double derive des tests du site"
```

---

## Task 5 : le coffre-fort

**Files:**
- Create: `spike/crates/sky-compte/src/coffre.rs`
- Modify: `spike/crates/sky-crypto/src/lib.rs` (sérialisation de la clé privée)
- Modify: `spike/crates/sky-compte/Cargo.toml` (`keyring = "3"`)

**Interfaces:**
- Consumes : `Identity` de `sky-crypto`.
- Produces :
  - `sky_crypto::Identity::depuis_octets(&[u8; 32]) -> Identity` et
    `Identity::en_octets(&self) -> [u8; 32]`
  - `Coffre::nouveau() -> Result<Coffre, ErreurCompte>`
  - `Coffre::pour_test(prefixe: &str) -> Coffre` — nom de service distinct par test, et
    nettoyage derrière lui. **Toutes** les tâches suivantes l'utilisent dans leurs tests ;
    sans lui, les tests se marcheraient dessus dans le trousseau réel de la machine.
  - `Coffre::identite(&self) -> Result<Identity, ErreurCompte>` — génère et range au
    premier appel, relit ensuite
  - `Coffre::jetons(&self) -> Result<Option<Jetons>, ErreurCompte>`,
    `Coffre::ranger_jetons(&self, &Jetons)`, `Coffre::oublier(&self)`
  - `pub struct Jetons { pub session: String, pub renouvellement: String }`

- [ ] **Étape 1 : écrire le test qui échoue**

```rust
#[test]
fn l_identite_survit_a_un_redemarrage() {
    // La cle privee ne doit PAS etre ephemere : c'est ce que le commentaire
    // de sky-crypto annonce depuis le jalon 0 (« au jalon 1 elle ira dans le
    // coffre-fort du systeme »). Une cle regeneree a chaque lancement
    // invaliderait toutes les enveloppes en vol.
    let coffre = Coffre::pour_test("sky-test-identite");
    let premiere = coffre.identite().unwrap().public_key();
    drop(coffre);

    let coffre = Coffre::pour_test("sky-test-identite");
    assert_eq!(coffre.identite().unwrap().public_key(), premiere);
}

#[test]
fn un_aller_retour_par_les_octets_preserve_la_cle() {
    let a = Identity::generate();
    let b = Identity::depuis_octets(&a.en_octets());
    let scelle = Identity::generate().seal(&b.public_key(), b"secret");
    assert_eq!(a.open(&scelle).unwrap(), b"secret");
}
```

- [ ] **Étape 2 : lancer, vérifier l'échec**

```bash
cd spike && cargo test -p sky-compte coffre
```

- [ ] **Étape 3 : implémenter**

`Coffre::pour_test(prefixe)` utilise un nom de service distinct par test, et nettoie
derrière lui — sinon les tests se marchent dessus dans le trousseau réel de la machine.

**Ne jamais journaliser ni afficher** le contenu du coffre. `Jetons` ne dérive **pas**
`Debug` ; si un `Debug` est nécessaire, l'écrire à la main pour qu'il n'affiche rien.

- [ ] **Étape 4 : lancer les tests**

```bash
cd spike && cargo test -p sky-compte && cargo clippy -p sky-compte -- -D warnings
```

- [ ] **Étape 5 : prouver par neutralisation**

Faire renvoyer à `Coffre::identite` une `Identity::generate()` neuve à chaque appel.
Attendu : `l_identite_survit_a_un_redemarrage` rougit. Rétablir.

- [ ] **Étape 6 : commiter**

```bash
git add -A && git commit -m "feat: coffre-fort systeme pour la cle privee et les jetons"
```

---

## Task 6 : la connexion native

**Files:**
- Create: `spike/crates/sky-compte/src/session.rs`
- Modify: `spike/crates/sky-compte/Cargo.toml` (`tiny_http = "0.12"` en dépendance
  ordinaire cette fois, `rand` pour le secret)

**Interfaces:**
- Consumes : `ClientHttp`, `Config`, `ErreurCompte` (T3) ; `Coffre`, `Jetons` (T5).
- Produces :
  - `pub fn connecter(config: &Config, coffre: &Coffre) -> Result<Jetons, ErreurCompte>`
  - `pub fn url_de_depart(port: u16, empreinte: &str) -> String`
  - `pub fn empreinte_du_secret(secret: &str) -> String` — SHA-256 en hexadécimal minuscule
  - `pub fn secret_aleatoire() -> String` — 32 octets tirés de l'OS, en hexadécimal
  - `pub fn echanger_le_code(config: &Config, code: &str, secret: &str) -> Result<Jetons, ErreurCompte>`
    — le `POST /api/auth/native` seul, isolé pour être testable sans navigateur

- [ ] **Étape 1 : écrire le test qui échoue**

```rust
#[test]
fn l_url_de_depart_porte_le_port_et_l_empreinte() {
    let u = url_de_depart(47821, &"a".repeat(64));
    assert!(u.contains("/api/auth/discord?"));
    assert!(u.contains("port=47821"));
    assert!(u.contains(&format!("empreinte={}", "a".repeat(64))));
}

#[test]
fn le_secret_ne_quitte_jamais_la_machine_avant_l_echange() {
    // Le site ne recoit que l'EMPREINTE au depart. Le secret lui-meme ne part
    // qu'au POST final. C'est ce qui rend un code intercepte inutilisable.
    let secret = secret_aleatoire();
    let u = url_de_depart(1234, &empreinte_du_secret(&secret));
    assert!(!u.contains(&secret));
}

#[test]
fn un_code_refuse_ne_range_rien_dans_le_coffre() {
    let s = FauxServeur::demarrer();
    s.etat_mut().refuser_echange = true;
    let coffre = Coffre::pour_test("sky-test-refus");

    let r = echanger_le_code(&Config::vers(&s.url()), "code-bidon", "secret-bidon");

    assert!(matches!(r, Err(ErreurCompte::Refuse)));
    assert!(coffre.jetons().unwrap().is_none());
}
```

- [ ] **Étape 2 : lancer, vérifier l'échec**

```bash
cd spike && cargo test -p sky-compte session
```

- [ ] **Étape 3 : implémenter**

Le déroulement : tirer un `secret` au hasard (32 octets, hexadécimal), calculer son
empreinte SHA-256, ouvrir un `tiny_http` sur le **port 0** (le système en attribue un
libre), lire le port réel, ouvrir le navigateur sur `url_de_depart(port, empreinte)`,
attendre **une seule** requête, en extraire `code` **ou** `error`, fermer le serveur, puis
`POST /api/auth/native { code, secret }`.

Le serveur de boucle locale rend une page qui dit à l'utilisateur de retourner dans le
terminal, et **ne sert qu'une requête** : il ferme aussitôt.

- [ ] **Étape 4 : lancer les tests**

```bash
cd spike && cargo test -p sky-compte && cargo clippy -p sky-compte -- -D warnings
```

- [ ] **Étape 5 : commiter**

```bash
git add -A && git commit -m "feat: connexion native par boucle locale"
```

---

## Task 7 : le renouvellement de jeton

Le renouvellement existe côté site et **a échoué pour 100 % des tentatives réelles pendant
tout un jalon**, faute d'appelant — le défaut n'a été trouvé que par une relecture. C2 en
est le premier appelant réel.

**Files:**
- Modify: `spike/crates/sky-compte/src/session.rs`

**Interfaces:**
- Produces : `pub fn renouveler(config: &Config, coffre: &Coffre) -> Result<Jetons, ErreurCompte>`
  et `pub fn jeton_valide(config: &Config, coffre: &Coffre) -> Result<String, ErreurCompte>`
  — rend le jeton de session, en le renouvelant d'abord si le serveur l'a rejeté.

- [ ] **Étape 1 : écrire le test qui échoue**

Ces tests appellent **`jeton_valide` directement**, pas `synchroniser` : l'annuaire n'existe
qu'à la tâche 8, et un test qui référence une fonction d'une tâche ultérieure ne compile pas.

```rust
#[test]
fn un_401_declenche_un_renouvellement_puis_une_seule_reprise() {
    // Par le VRAI chemin : le jeton presente vient du coffre, pas d'une
    // charge fabriquee a la main. C'est precisement la charge fabriquee a la
    // main qui etait correcte au jalon C1, et celle du vrai chemin qui ne
    // l'etait pas.
    let s = FauxServeur::demarrer();
    s.etat_mut().refuser_le_premier_appel = true;
    let coffre = Coffre::pour_test("sky-test-renouv");
    coffre.ranger_jetons(&Jetons { session: "perime".into(), renouvellement: "bon".into() }).unwrap();

    let jeton = jeton_valide(&Config::vers(&s.url()), &coffre).unwrap();

    assert_eq!(jeton, "jeton-neuf");
    assert_eq!(s.etat_mut().appels_de_renouvellement, 1, "un seul renouvellement");
    assert_eq!(coffre.jetons().unwrap().unwrap().session, "jeton-neuf");
}

#[test]
fn un_renouvellement_refuse_ne_boucle_pas() {
    // Sans cette garantie, un jeton de renouvellement revoque produit une
    // boucle infinie d'appels au serveur.
    let s = FauxServeur::demarrer();
    s.etat_mut().refuser_tout = true;
    let coffre = Coffre::pour_test("sky-test-boucle");
    coffre.ranger_jetons(&Jetons { session: "x".into(), renouvellement: "y".into() }).unwrap();

    let r = jeton_valide(&Config::vers(&s.url()), &coffre);

    assert!(matches!(r, Err(ErreurCompte::Refuse)));
    assert!(s.etat_mut().appels_de_renouvellement <= 1);
}
```

- [ ] **Étape 2 : lancer, vérifier l'échec**

```bash
cd spike && cargo test -p sky-compte renouvellement
```

- [ ] **Étape 3 : implémenter — une seule reprise, jamais deux**

- [ ] **Étape 4 : lancer les tests et commiter**

```bash
cd spike && cargo test -p sky-compte
git add -A && git commit -m "feat: renouvellement de jeton, une seule reprise"
```

---

## Task 8 : l'annuaire

**Files:**
- Create: `spike/crates/sky-compte/src/annuaire.rs`

**Interfaces:**
- Consumes : `ClientHttp`, `Config`, `ErreurCompte` (T3) ; `jeton_valide` (T7).
- Produces :
  - `pub struct Etat { pub version: u64, pub code: String, pub amis: Vec<Ami>, pub appareils: Vec<Appareil> }`
  - `pub struct Ami { pub id: i64, pub discord_name: String, pub appareils: Vec<AppareilDAmi> }`
  - `pub struct AppareilDAmi { pub id: i64, pub public_key: String }` — base64
  - `pub fn synchroniser(&Config, &Coffre) -> Result<Etat, ErreurCompte>`
  - `pub fn enregistrer_appareil(&Config, &Coffre, nom: &str, cle: &[u8; 32]) -> Result<i64, ErreurCompte>`
  - `pub fn ajouter_ami(&Config, &Coffre, code: &str) -> Result<(), ErreurCompte>`
  - `pub fn accepter_ami(&Config, &Coffre, friendship_id: i64) -> Result<(), ErreurCompte>`
  - `pub fn resoudre_ami(etat: &Etat, designation: &str) -> Result<&Ami, ErreurCompte>`

- [ ] **Étape 1 : écrire le test qui échoue**

```rust
#[test]
fn deux_amis_de_meme_nom_font_refuser_plutot_que_choisir() {
    // Se tromper de destinataire, ici, veut dire sceller ses adresses pour
    // la mauvaise personne. On refuse et on demande l'identifiant.
    let etat = etat_avec(vec![ami(1, "bob"), ami(2, "bob")]);
    let r = resoudre_ami(&etat, "bob");
    assert!(matches!(r, Err(ErreurCompte::Protocole(_))));
    assert!(resoudre_ami(&etat, "2").unwrap().id == 2);
}

#[test]
fn un_ami_sans_appareil_donne_un_tableau_vide_pas_une_erreur() {
    let etat = etat_avec(vec![ami_sans_appareil(1, "bob")]);
    assert!(resoudre_ami(&etat, "bob").unwrap().appareils.is_empty());
}

#[test]
fn la_cle_publique_fait_bien_32_octets_apres_decodage() {
    // Une cle tronquee ne se verrait qu'au moment du scellage, loin d'ici.
    let etat = etat_avec(vec![ami(1, "bob")]);
    let brut = base64_decoder(&resoudre_ami(&etat, "bob").unwrap().appareils[0].public_key);
    assert_eq!(brut.len(), 32);
}
```

- [ ] **Étape 2 : lancer, vérifier l'échec**

```bash
cd spike && cargo test -p sky-compte annuaire
```

- [ ] **Étape 3 : implémenter**

`Etat.version` sert au paramètre `?version=` de la synchronisation ; une réponse
`{ inchange: true }` conserve l'état précédent.

- [ ] **Étape 4 : lancer les tests et commiter**

```bash
cd spike && cargo test -p sky-compte
git add -A && git commit -m "feat: annuaire — synchronisation, amis, appareils"
```

---

## Task 9 : la boîte aux lettres

**Files:**
- Create: `spike/crates/sky-compte/src/boite.rs`

**Interfaces:**
- Consumes : `Identity` (`seal`/`open`) de `sky-crypto` ; `AppareilDAmi`, `Etat` (T8) ;
  `jeton_valide` (T7).
- Produces :
  - `pub fn deposer(&Config, &Coffre, destinataires: &[AppareilDAmi], charge_claire: &[u8]) -> Result<usize, ErreurCompte>`
  - `pub fn relever(&Config, &Coffre, identite: &Identity) -> Result<Vec<Message>, ErreurCompte>`
  - `pub struct Message { pub id: String, pub expediteur_device_id: i64, pub clair: Vec<u8> }`
  - `pub const TAILLE_MAX_CHARGE: usize = 4096;`

- [ ] **Étape 1 : écrire le test qui échoue**

```rust
#[test]
fn une_charge_trop_grosse_est_refusee_avant_le_reseau() {
    // La base impose 4096 octets (contrainte envelopes_taille). Refuser ici
    // coute zero aller-retour et donne un message clair, au lieu d'un rejet
    // opaque du serveur.
    let s = FauxServeur::demarrer();
    let r = deposer(&Config::vers(&s.url()), &coffre_de_test(), &[appareil_bidon()],
                    &vec![0u8; 5000]);
    assert!(matches!(r, Err(ErreurCompte::Protocole(_))));
    assert_eq!(s.etat_mut().depots_recus, 0, "aucun appel reseau");
}

#[test]
fn le_scellage_precede_jamais_la_compression() {
    // Un contenu chiffre est indistinguable du hasard et NE SE COMPRIME PAS.
    // Inverser les deux ferait exploser la taille sans que rien n'echoue.
    let sdp = "v=0\r\n".repeat(200);
    let comprime = sky_net::handshake::comprimer(&sdp);
    assert!(comprime.len() < sdp.len() / 2, "la compression doit mordre");
}

#[test]
fn un_tiers_ne_peut_pas_ouvrir_ce_qui_ne_lui_est_pas_destine() {
    let alice = Identity::generate();
    let bob = Identity::generate();
    let mallory = Identity::generate();
    let scelle = alice.seal(&bob.public_key(), b"offre");
    assert!(mallory.open(&scelle).is_err());
}
```

- [ ] **Étape 2 : lancer, vérifier l'échec**

```bash
cd spike && cargo test -p sky-compte boite
```

- [ ] **Étape 3 : implémenter**

`deposer` scelle **une fois par destinataire** — la boîte scellée tire une paire éphémère à
chaque appel, donc deux scellages du même message diffèrent, et c'est voulu.

`relever` ignore en silence les enveloppes qu'il ne sait pas ouvrir : elles peuvent être
destinées à un autre appareil, ou avoir été déposées par un ami retiré depuis. Ce n'est pas
une erreur, et un commentaire doit le dire.

- [ ] **Étape 4 : lancer les tests et commiter**

```bash
cd spike && cargo test -p sky-compte
git add -A && git commit -m "feat: boite aux lettres — depot et releve d enveloppes scellees"
```

---

## Task 10 : les sous-commandes

**Files:**
- Create: `spike/crates/sky-probe/src/cmd_compte.rs`
- Modify: `spike/crates/sky-probe/src/main.rs`
- Modify: `spike/crates/sky-probe/Cargo.toml` (dépendance vers `sky-compte`)

**Interfaces:**
- Consumes : tout `sky-compte`.
- Produces : les sous-commandes `login`, `device register|list`,
  `friends add|accept|list`, `code`.

- [ ] **Étape 1 : ajouter les variantes**

`main.rs` suit le style `clap` derive déjà en place — une variante d'`enum Cmd` avec un
commentaire `///` qui devient l'aide. Les nouvelles variantes s'ajoutent au même endroit,
dans le même style, et le `match` de `main` les route vers `cmd_compte::*`.

- [ ] **Étape 2 : écrire le test qui échoue**

```rust
#[test]
fn le_message_d_echec_de_connexion_est_unique() {
    // Le serveur refuse sans distinguer code inconnu, expire, deja consomme
    // ou mauvais secret. L'affichage ne doit pas reintroduire la distinction.
    assert_eq!(message_utilisateur(&ErreurCompte::Refuse),
               "Connexion refusee. Relance `sky-probe login`.");
}

#[test]
fn aucun_jeton_n_est_jamais_affiche() {
    let jetons = Jetons { session: "SECRET-SESSION".into(), renouvellement: "SECRET-RENOUV".into() };
    let sortie = resume_de_connexion(&jetons, "Killian");
    assert!(!sortie.contains("SECRET"));
    assert!(sortie.contains("Killian"));
}
```

- [ ] **Étape 3 : lancer, vérifier l'échec, implémenter, relancer**

```bash
cd spike && cargo test -p sky-probe && cargo clippy -- -D warnings
```

- [ ] **Étape 4 : commiter**

```bash
git add -A && git commit -m "feat: sous-commandes de compte dans sky-probe"
```

---

## Task 11 : le copier-coller disparaît

La tâche qui rend le jalon vrai.

**Files:**
- Modify: `spike/crates/sky-probe/src/cmd_host.rs:131` (le `stdin().read_line`)
- Modify: `spike/crates/sky-probe/src/cmd_view.rs:51` (le `stdin().read_line`)

**Interfaces:**
- Consumes : `PeerLink::offrant`/`repondant`/`accepter_reponse` (T1) ; `synchroniser`,
  `resoudre_ami` (T8) ; `deposer`, `relever` (T9) ; `Coffre::identite` (T5).

- [ ] **Étape 1 : les constantes de cadence, d'un seul endroit**

```rust
/// Points de depart argumentes, PAS des mesures : 2 s est sous le seuil ou
/// une attente se remarque, 30 min couvre une session de jeu sans courir
/// indefiniment, 60 s suffit a un hote deja disponible. A ajuster apres le
/// premier essai reel plutot qu'a deviner deux fois.
const CADENCE: Duration = Duration::from_secs(2);
const FENETRE_HOTE: Duration = Duration::from_secs(30 * 60);
const ATTENTE_SPECTATEUR: Duration = Duration::from_secs(60);
```

- [ ] **Étape 2 : `cmd_view` — le spectateur offre**

Synchroniser, résoudre l'ami, **produire l'offre** par `PeerLink::offrant`, la comprimer,
la sceller pour **chacun** des appareils non révoqués de l'ami, déposer. Puis relever toutes
les `CADENCE` pendant `ATTENTE_SPECTATEUR` jusqu'à trouver la réponse, et
`accepter_reponse`.

Au bout du délai, dire franchement : « Bob n'a pas répondu — est-il en partage ? »

- [ ] **Étape 3 : `cmd_host` — l'hôte répond**

Relever toutes les `CADENCE` pendant `FENETRE_HOTE`. À la première enveloppe ouverte :
décomprimer l'offre, `PeerLink::repondant`, comprimer et sceller la réponse **pour la clé
publique de l'expéditeur** — celle que `PeerLink::peer_key()` expose depuis l'offre reçue —
puis déposer.

L'identifiant de session de 4 octets du `Blob` est conservé : l'offre le tire, la réponse le
recopie. Sans lui, une réponse d'un essai précédent produit « mauvaise clé ou message
altéré », un message qui envoie chercher un problème de chiffrement inexistant.

- [ ] **Étape 4 : vérifier que rien ne lit plus l'entrée standard**

```bash
cd spike && grep -rn "stdin" crates/sky-probe/src/cmd_host.rs crates/sky-probe/src/cmd_view.rs
```
Attendu : **aucune ligne**. C'est le critère littéral du jalon.

- [ ] **Étape 5 : lancer tout**

```bash
cd spike && cargo test && cargo clippy -- -D warnings && cargo run -p sky-probe -- selftest
```

- [ ] **Étape 6 : l'essai réel — le jalon n'est pas clos sans lui**

Deux machines, deux réseaux, **deux comptes Discord distincts**. Les deux s'ajoutent en ami
par leur code, acceptent, enregistrent leur appareil. L'un lance `host`, l'autre `view`.

À relever et consigner : le délai entre le lancement de `view` et l'ouverture du canal ; le
nombre d'appels à la synchronisation de chaque côté ; et si une enveloppe est restée
non ouverte.

**Cette étape mobilise une deuxième personne : le contrôleur l'organise avec le
propriétaire, l'implémenteur ne peut pas la faire seul.**

- [ ] **Étape 7 : commiter**

```bash
git add -A && git commit -m "feat: la connexion se negocie par la boite aux lettres"
```

---

## Auto-relecture du plan

**Couverture de la spec.** D1 → T3/T10 (crate séparé, CLI qui ne fait qu'appeler) ·
D2 → T11 étape 1 (les trois cadences) · D3 → T2 · D4 → T2 étapes 3 et 5 · D5 → T1 ·
D6 → contraintes globales (pas de `tokio`) · D7 → T5 · D8 → T4 et T11 étape 6.
§7 (erreurs) → T3 (délai de 5 s), T7 (renouvellement), T10 (message unique).
§6 (dette `sontAmis`) → T2 étape 6. Aucune section de la spec sans tâche.

**Cohérence des types — quatre défauts trouvés et corrigés à la relecture**, du même genre
que les huit que les agents ont trouvés dans mon plan du C1 :

1. **Référence en avant, la plus grave.** Les tests de T7 appelaient `synchroniser()`, qui
   n'existe qu'à T8. Ça ne compile pas. Réécrits pour appeler `jeton_valide` directement.
2. `Config::vers()` était utilisée dans les tests de quatre tâches sans être déclarée nulle
   part. Ajoutée aux sorties de T3.
3. `Coffre::pour_test()` apparaissait dans une étape d'implémentation de T5 et dans les
   tests de trois tâches, mais pas dans ses interfaces. Ajoutée.
4. `secret_aleatoire()` et `echanger_le_code()` étaient utilisées dans les tests de T6 sans
   y être produites. Ajoutées.

**Risque assumé.** T1 est placée en premier parce que c'est la seule dont l'échec
remettrait le jalon en cause, et `selftest` la valide sans réseau ni deuxième personne.

**Point que ce plan ne tranche pas, et qui reviendra.** Si l'essai réel de T11 montre que
les trois cadences sont mauvaises, ce sont des constantes d'un seul endroit — l'ajustement
est une ligne, pas une reprise.
