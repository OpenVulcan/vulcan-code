Vulcan Code 0.1.7 permet de lancer l'application de bureau Linux depuis le dock et le menu des applications sous Ubuntu 23.10 et versions ultérieures, rend les réinstallations et les mises à jour sous Windows plus sûres, corrige plusieurs failles de sécurité et pertes de données, et inclut le programme d'installation 0.2.9.

### Mettre à jour vers cette version

```ui-checklist
ok | Mise à jour complète | Cette version modifie l'installateur et la structure de l'installation ; elle s'installe donc comme une mise à jour complète : sous Windows, l'application télécharge et exécute l'installateur une fois
ok | Installations par setup | Les installations faites avec le programme d'installation se mettent à jour avec vulcan-code-setup update ; l'application l'indique au lieu de proposer un téléchargement
```

### Linux

```ui-checklist
ok | Dock et menu des applications | Sous Ubuntu 23.10 et versions ultérieures, setup 0.2.9 installe une fois un profil AppArmor, si bien que l'application de bureau démarre depuis le dock et le menu des applications, et pas seulement depuis une icône du bureau
ok | Profil depuis l'application | Si le profil manque ou est obsolète, les réglages de mise à jour proposent de l'installer en demandant une seule fois le mot de passe administrateur, ou affichent la commande à exécuter dans un terminal
```

### Windows

```ui-checklist
ok | Réinstallation sûre | Installer dans un dossier sans installation précédente ne supprime jamais ses fichiers ; si la réinstallation de la même version n'aboutit pas, la précédente est restaurée
ok | Lancement d'essai | Ouvrir l'application de bureau pendant que le Core précédent tourne encore ne compte plus comme un échec de démarrage de la nouvelle version et ne la fait plus revenir en arrière
```

### Sécurité

```ui-checklist
ok | Service local | Le Core n'accepte que les requêtes adressées à un nom d'hôte de bouclage ; une page web qui fait pointer sa propre adresse vers cet ordinateur ne peut donc pas lire le service
ok | Pièces jointes | Les pièces jointes SVG s'ouvrent dans un bac à sable et ne peuvent pas exécuter de script, et leurs chemins ne peuvent pas sortir de leur dossier
ok | Contrôle de lecture seule | Les outils d'extension dont le nom contient un verbe d'écriture, les options d'écriture de git branch et les options d'exécution de fd ne sont plus traités comme en lecture seule
ok | Secrets | Enregistrer une valeur ordinaire sur une variable d'environnement secrète la laisse secrète
```

### Corrections

```ui-checklist
ok | Skills et projets | Installer un skill local depuis son propre dossier ne le supprime plus, et supprimer un projet supprime aussi ses variables de projet
ok | Mémoire | La mémoire Markdown lit correctement les titres en chinois, japonais et coréen
ok | Mode plan | Activer ou désactiver le mode plan après la fin d'un tour n'échoue plus
ok | Paramètres des outils | Les paramètres qui renvoient à des définitions partagées, comme des listes de valeurs autorisées, restent visibles et obligatoires
ok | Paramètres du modèle | Les paramètres décimaux sont envoyés comme des nombres, et l'extraction Tavily fonctionne avec des URL seules
ok | Modification de fichiers | Une insertion en fin de fichier conserve sa dernière ligne et les lignes vides finales
ok | WebUI | Les flèches dans le message de commit déplacent le curseur, les lignes de diff qui commencent par --- ou +++ s'affichent, et les exemples de code gardent leurs liens d'image tels qu'écrits
```

### Client terminal

```ui-checklist
ok | Messages de suivi | Les messages envoyés à une session existante sont acceptés, et un message refusé revient dans la zone de saisie
ok | Git | Une connexion perdue ne bloque plus le commit ni la génération du message jusqu'au redémarrage
ok | Niveaux de réflexion | Chaque niveau de réflexion est envoyé tel qu'il a été choisi
```

### Programme d'installation

```ui-checklist
ok | Profil AppArmor | Setup 0.2.9 installe le profil via sudo lorsque le système en a besoin et laisse la commande exacte dans les notes si le mot de passe n'est pas saisi ; la désinstallation le supprime
```
