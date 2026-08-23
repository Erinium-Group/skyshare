# Test SkyShare — 5 minutes

Salut, et merci de tester. Aucune installation, aucun compte, rien à
configurer. Tu lances un fichier, tu copies deux blocs de texte, c'est fini.

## Ce que ça fait

Ton ordinateur et le mien essaient de se parler **directement**, sans passer par
le serveur de qui que ce soit. C'est la promesse de SkyShare, et personne ne sait
encore si elle tient : ça dépend de vos deux box Internet. Ce test répond à cette
question, et à rien d'autre.

## Ce que ça envoie, et ce que ça n'envoie pas

Le programme envoie **deux choses seulement** :

1. Une question à un serveur public de Google ou de Cloudflare, pour lui demander
   « quelle adresse vois-tu de moi ? ». C'est la seule façon de savoir comment
   te joindre depuis l'extérieur. Cette question ne contient rien sur toi.
2. Un bloc de texte que **tu me renvoies toi-même**, à la main. Il contient
   l'adresse réseau de ton ordinateur, chiffrée : je suis le seul à pouvoir la
   lire, parce qu'elle est verrouillée avec une clé que seul mon programme
   possède.

Ce que le programme ne fait **pas** : il ne lit aucun fichier, ne capture aucun
écran, n'ouvre aucune fenêtre, n'installe rien, ne démarre avec Windows, ne
touche à aucun réglage. Il ne s'écrit nulle part sur ton disque. Quand il se
ferme, il n'en reste rien. Tu peux supprimer le fichier après le test.

Pendant ce test, aucune image de ton écran n'est capturée ni transmise.

## Marche à suivre

1. Télécharge `sky-probe.exe` et mets-le où tu veux, par exemple sur le Bureau.
2. Ouvre un terminal dans ce dossier : clic droit sur le dossier →
   « Ouvrir dans le Terminal ».
3. Tape ceci, puis Entrée :

       .\sky-probe.exe view

4. Je t'envoie un long bloc de texte qui commence par `SKY1:`. Copie-le **en
   entier**, colle-le dans le terminal (clic droit = coller), puis Entrée.
5. Le programme t'affiche à son tour un bloc `SKY1:`. Renvoie-le-moi **en
   entier**, d'un seul bloc, sans le couper.
6. **Laisse la fenêtre ouverte et ne touche à rien.** Le programme affiche
   `toujours en attente` toutes les 30 secondes : c'est normal, il attend que je
   colle ton bloc de mon côté. Ça peut prendre une ou deux minutes, le temps que
   je voie ton message.
7. Quand j'ai collé ton bloc, tout se joue en quelques secondes : l'écran
   affiche `CONNECTÉ`, puis des chiffres de débit — ou bien `ÉCHEC`.

Si tu fermes la fenêtre avant que j'aie collé ton bloc, le test est annulé et il
faut tout recommencer. C'est sans gravité, mais autant l'éviter.

Envoie-moi une capture d'écran du résultat, quel qu'il soit.

## Si le bloc arrive coupé

C'est le raté le plus probable de tout ce test. Le bloc fait environ 3 800
caractères : certaines messageries le coupent en plusieurs morceaux, ou insèrent
des retours à la ligne au milieu. Le programme dira alors quelque chose comme
« bloc illisible — la copie est probablement incomplète ».

Dans ce cas : envoie-le-moi **en pièce jointe**, dans un simple fichier `.txt`.
C'est la façon la plus sûre. Un envoi par courriel fonctionne aussi. Ce qu'il
faut éviter, c'est de le retaper à la main ou de le recoller morceau par morceau.

## « Windows a protégé votre ordinateur »

Ce message va apparaître, et c'est normal : le fichier n'est pas signé
électroniquement, parce que la signature coûte plusieurs centaines d'euros par an
et que ce programme est un brouillon qui ne sortira jamais de ce test.

Pour passer outre : clique sur **« Informations complémentaires »**, puis sur
**« Exécuter quand même »**.

Ton antivirus peut aussi râler, pour la même raison. Si tu n'es pas à l'aise avec
ça, dis-le-moi simplement — c'est une réaction saine, et je ne t'en voudrai pas
une seconde.

## Si ça échoue

**Un échec m'apprend autant qu'un succès.** Ce test existe justement parce que je
ne sais pas si ça marche. Si l'écran affiche `ÉCHEC`, ce n'est ni ta faute ni un
problème de ton ordinateur, et c'est exactement l'information que je cherche.
Elle décidera de la suite du projet.

En revanche, ne cherche pas à interpréter toi-même la cause : le programme le
fait mieux que nous deux. Il y a plusieurs raisons possibles — l'une de nos deux
box qui refuse les connexions directes, mais aussi une négociation qui aboutit
puis se casse plus loin, ou simplement un bloc parti trop tard. Le texte affiché
sous le mot `ÉCHEC` dit laquelle, et il prend soin de ne pas accuser au hasard.

C'est pour ça que **la capture d'écran compte plus que ton résumé** : envoie-moi
l'écran entier, avec toutes les lignes, plutôt que « ça n'a pas marché ». Et
surtout, ne recommence pas dix fois en pensant avoir mal fait — une seule
tentative, réussie ou non, est parfaite comme ça.

Merci.
