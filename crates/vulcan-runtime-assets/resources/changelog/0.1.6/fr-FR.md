Vulcan Code 0.1.6 corrige le redémarrage après une mise à jour et la réouverture de l’application sous Linux, permet de fermer une fenêtre de bureau dont le démarrage a échoué et s’accompagne de l’installateur 0.2.8, qui mène toute l’installation dans l’installateur interactif.

### Bureau

```ui-checklist
ok | Échec du démarrage | Si le démarrage ou le chargement de l’interface échoue, la fenêtre affiche des boutons pour recharger et fermer sous Windows et Linux, afin de pouvoir la fermer
ok | Répertoire de données occupé | Si un autre processus occupe le répertoire de données sans répondre, la page de démarrage indique de terminer les processus vulcan-code-service puis de rouvrir l’application
```

### Linux

```ui-checklist
ok | Redémarrage après mise à jour | Après une mise à jour, le Core redémarre aussitôt ; l’application de bureau s’ouvre sur la nouvelle version sans attendre deux minutes
ok | Réouverture | Ouvrir l’application pendant sa fermeture la relance au lieu de ne rien faire, et les processus auxiliaires de l’application de bureau ne subsistent plus
ok | Icône du dock | L’entrée de menu de l’installateur 0.2.8 correspond à la fenêtre de l’application, si bien qu’une icône épinglée au dock lui appartient ; lancer une fois la mise à jour de l’installateur réécrit une entrée existante
```

### Corrections

```ui-checklist
ok | Erreurs du fournisseur | Lorsqu’un fournisseur signale une erreur au milieu d’une réponse, son propre message s’affiche au lieu d’un avis indiquant qu’un élément de réponse était encore ouvert (protocoles OpenAI Chat et Anthropic)
```

### Installateur

```ui-checklist
ok | Installateur interactif | Après l’assistant, le récapitulatif, la progression du téléchargement et le résultat restent dans l’installateur interactif, dans la langue de l’interface ; les mises à jour l’utilisent aussi
ok | Téléchargement exact | Le récapitulatif indique exactement la quantité à télécharger ; les fichiers déjà présents ne sont pas recomptés
ok | La prochaine fois | La dernière page explique comment modifier, mettre à jour ou désinstaller l’installation plus tard, et que l’installateur se met d’abord à jour lui-même
ok | Annuler | Revenir en arrière ou annuler au récapitulatif ne change rien et se termine sans erreur
ok | Scripts d’installation | Les scripts d’installation affichent la source et la progression du téléchargement, et passent à un autre serveur après 30 secondes sans données
```
