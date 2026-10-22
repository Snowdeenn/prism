# À quoi servent les nouvelles structures de Prism ?

Cette note présente les structures de Prism V2 et leur rôle dans une application. Elle décrit l’API actuellement présente dans le dépôt. Pour les signatures et les exemples détaillés, voir le [guide V2](V2_GUIDE.md).

## L’idée générale

L’application décide **quoi afficher** : objets, caméra, paramètres et effets actifs. Prism organise **comment produire l’image sur le GPU**.

La V2 sépare les ressources réutilisables, la description d’une image et les étapes de rendu. Cette séparation permet de réutiliser les mêmes géométries et matériaux dans plusieurs images, et d’ajouter des passes personnalisées.

## 1. Initialiser et conserver le moteur

### `GpuContext` — la connexion au GPU et à la fenêtre

Il conserve le device, la queue, la surface de la fenêtre et sa configuration. Il sert aussi à transmettre les changements de taille avec `resize`.

On le crée à l’initialisation et on le conserve entre les images.

### `ResourceManager` — le stockage des ressources

Il conserve les shaders, buffers, textures, vues de textures, samplers, géométries, matériaux et ensembles de bindings.

Il renvoie des identifiants typés, appelés *handles*, pour les retrouver. Un `GeometryHandle` désigne une géométrie ; un `MaterialHandle` désigne un matériau. Les objets à dessiner utilisent ces références au lieu de recopier les ressources GPU.

Les handles sont générationnels : un ancien handle ne doit pas désigner une nouvelle ressource après réutilisation de son emplacement. Ils appartiennent au manager qui les a créés et ne doivent pas être transférés à un autre manager.

### `GraphRenderer` — l’exécution du rendu

Il prépare les ressources nécessaires, utilise les caches de pipelines, encode les commandes et les soumet au GPU. Il conserve aussi les effets de post-traitement enregistrés.

Pour une fenêtre, `render` passe par `GpuContext` pour acquérir et présenter l’image. Pour une texture hors écran, on utilise `render_to_texture`.

`GpuContext`, `ResourceManager` et `GraphRenderer` se conservent entre les images et doivent utiliser le même device. Si le device est recréé, il faut recréer les services V2.

## 2. Décrire ce qu’on dessine

### `Geometry` — les données et la plage du dessin

Elle référence les buffers de sommets, un éventuel buffer d’indices et la plage à dessiner. `VertexBuffer` précise le slot et la plage d’octets ; `IndexBuffer` précise aussi le format des indices.

Exemple : un rectangle possède une géométrie réutilisable, que plusieurs objets peuvent dessiner avec des matériaux différents.

### `MaterialTemplate` — la définition commune d’un matériau

Il décrit les shaders, leurs points d’entrée, le schéma des bindings, le format des sommets et les états de rendu : transparence, profondeur, cibles couleur, etc.

Il est immuable une fois enregistré. Plusieurs matériaux peuvent partager le même template.

`ShaderEntry` désigne un shader, son point d’entrée et ses constantes de spécialisation.

### `Material` — les valeurs et ressources d’un matériau

Il référence un template et les bindings à utiliser. Il peut aussi contenir un `ParameterBlock` : des octets de paramètres destinés à un emplacement uniforme précis.

Exemple : deux sprites partagent le même template, mais leurs matériaux utilisent des textures ou des paramètres différents.

### `RenderObject` — une demande de dessin

Il associe une géométrie et un matériau, une plage d’instances, des bindings propres à l’objet, une phase et une clé de tri.

Ce n’est pas une entité de jeu : position, animation et logique restent dans l’application. Celle-ci fournit les données dont les shaders ont besoin.

`instances: 0..1` demande une instance. Une plage plus grande permet un dessin instancié, à condition de fournir les données correspondantes. Prism ne fusionne pas automatiquement les objets en lots.

`Bounds` décrit des limites spatiales optionnelles. Elles ne déclenchent actuellement aucun culling automatique.

## 3. Brancher les données sur les shaders

### `BindingSchema`, `GroupSchema`, `BindingEntryDesc` et `BindingKind`

Ces structures décrivent **ce que le shader attend** : numéros de groupes et de bindings, types de ressources et étapes du shader qui y accèdent.

Exemple : le shader attend un buffer de caméra dans un groupe, puis une texture et un sampler dans un autre.

### `BindingSet` et `BindingResource`

Ils décrivent **les ressources réellement fournies** à un groupe : buffer, vue de texture ou sampler.

Le schéma est le contrat ; le set fournit les ressources qui doivent respecter ce contrat.

### `BindingSetUse`

Il référence un set et ajoute les offsets dynamiques nécessaires à son utilisation. Cela permet notamment de sélectionner une portion d’un buffer partagé.

Les bindings peuvent venir de la vue, du matériau et de l’objet. Les numéros de groupes sont explicites : les conventions de l’ancienne API ne sont pas des obligations de la V2.

## 4. Construire une image

### `FrameV2` — tout ce qu’on soumet pour une image

Elle contient les vues, les objets, les uploads et les effets de post-traitement demandés. Elle possède ses listes et ses octets d’upload ; les ressources persistantes sont référencées par handles.

On construit une nouvelle soumission pour chaque image. Les ressources du `ResourceManager` restent réutilisables.

### `ViewSubmission` — les données d’une vue

Elle contient les bindings de vue, les phases à dessiner et un viewport optionnel, exprimé en pixels. L’application calcule les matrices de caméra et les transmet via les bindings.

Une vue sélectionne des phases parmi les objets de la frame ; elle ne contient pas sa propre liste d’objets.

### `PhaseSubmission`, `RenderPhaseId`, `SortPolicy` et `SortKey`

Une phase regroupe les objets d’une catégorie. Les phases prévues par défaut sont les objets opaques, transparents et les overlays.

`PhaseSubmission` associe une phase à une politique de tri. `SortKey` est la clé fournie par l’application ; `SortPolicy` détermine comment les objets sont ordonnés. Il faut donc fournir des clés cohérentes avec l’ordre visuel recherché.

### `UploadRequest` — les données à copier vers le GPU

Elle demande une copie dans un buffer ou une texture. Les données sont conservées dans la frame et leurs copies sont encodées avant les passes du graphe.

Exemple : actualiser les données d’instances avant de dessiner les sprites. Les usages et alignements exigés par les copies GPU doivent être respectés.

### `PostProcessSubmission`, `PostProcessId` et `OverlayPolicy`

Un effet est enregistré dans le renderer, qui renvoie un `PostProcessId`. Une `PostProcessSubmission` demande son exécution avec des paramètres pour cette image. Les effets suivent l’ordre de la liste.

`OverlayPolicy` choisit si les overlays passent avant ou après le post-traitement. Par défaut, ils sont dessinés après : un HUD peut ainsi rester lisible au-dessus d’une scène traitée.

## 5. Organiser des étapes de rendu personnalisées

Le rendu standard construit déjà un graphe. Ces structures deviennent utiles quand on veut contrôler les étapes, ajouter du compute ou composer un rendu particulier.

### `RenderGraph` — les ressources logiques et leurs dépendances

Il décrit les ressources utilisées et les lectures/écritures de chaque passe. Il valide les dépendances, établit l’ordre d’exécution et écarte les passes inutiles au résultat demandé, sauf celles déclarées avec un effet de bord.

`ResourceVersion` identifie un état logique d’une ressource. Une nouvelle version permet de représenter une écriture et les lectures qui en dépendent ; elle ne signifie pas nécessairement une nouvelle texture physique.

`PassDescriptor` décrit une passe ; `AccessKind` précise ses accès. `TextureDesc` et `ResourceDesc` décrivent les ressources à allouer. `CompiledGraph` contient notamment l’ordre retenu et les durées de vie calculées.

### `FrameGraph` — le graphe accompagné de son implémentation

Il associe le `RenderGraph`, les ressources GPU importées et le code des passes. `GraphImports` relie les ressources logiques aux ressources externes fournies par l’application.

Pour une passe personnalisée, utiliser `FrameGraph::add_pass` : cette méthode enregistre à la fois sa déclaration et son implémentation.

### `GraphPass` — le contrat d’une passe personnalisée

Il définit trois étapes :

1. `declare` annonce les accès et dépendances.
2. `prepare` résout les ressources et prépare les pipelines et bindings.
3. `encode` écrit les commandes GPU.

`PrepareContext` donne accès aux services de préparation. `PassResources` donne accès aux ressources du graphe déclarées par la passe.

Les passes intégrées comprennent `ClearPass` pour effacer une cible, `PhasePass` pour dessiner une phase et `FullscreenPass` pour un traitement plein écran.

## 6. Réutiliser les configurations GPU et observer le résultat

### `PipelineKey`, `ComputePipelineKey` et `PipelineCache`

Une clé décrit la configuration qui identifie un pipeline. Le cache réutilise un pipeline déjà créé lorsqu’une configuration équivalente est demandée.

`ShaderRevisionKey` et `ShaderStageKey` identifient notamment le shader et sa révision. Un rechargement de shader réussi permet ainsi de remplacer les pipelines concernés lors des encodages suivants.

Changer les paramètres ou la texture d’un matériau ne change pas, à lui seul, la clé du pipeline.

### `LayoutCache` et `ResolvedLayout`

Ils permettent de réutiliser les layouts GPU construits à partir des schémas de bindings. Ils servent surtout à la préparation interne ou aux intégrations avancées.

### `EncodedFrame` — des commandes prêtes à être soumises

Elle contient un buffer de commandes et son rapport. Elle permet de séparer l’encodage de la soumission avec `encode_to_texture`, puis `submit`.

La soumission ne signifie pas que le GPU a terminé son travail.

### `FrameReport` et `RenderError` — comprendre ce qui s’est passé

Le rapport fournit les dessins préparés, les octets d’upload demandés, les passes actives, une estimation des textures temporaires, le temps CPU de préparation et les diagnostics d’objets.

Le temps de préparation n’est pas une mesure du temps GPU. `skipped_objects` contient des diagnostics : avec un matériau de secours, un objet peut avoir été dessiné malgré une erreur initiale.

`RenderError` apporte un contexte et un message. Une erreur sur un objet peut conduire à ignorer son dessin ; une erreur de graphe ou de passe peut interrompre l’encodage.

## Le parcours habituel dans une application

1. Créer `GpuContext`, puis `ResourceManager` et `GraphRenderer` avec `from_context`.
2. Enregistrer les shaders, buffers, textures, géométries, templates et matériaux réutilisables.
3. Pour chaque image, calculer les données de caméra et d’objets côté application.
4. Construire `FrameV2` avec les vues, uploads, objets et effets actifs.
5. Appeler `render` pour la fenêtre, ou `render_to_texture` pour une sortie hors écran.
6. Vérifier les erreurs et le `FrameReport`.

Un graphe personnalisé n’est nécessaire que lorsque le rendu standard ne couvre pas le besoin.

## Attention aux noms de l’ancienne API

Les deux API coexistent. `prism::Frame`, `prism::Material` et `prism::PipelineKey` désignent encore les types historiques.

Pour la V2, utiliser `prism::FrameV2`, `prism::material::Material` et `prism::pipeline::PipelineKey`. `ResourceManager` prend en charge les ressources V2 ; `GraphRenderer` est son point d’entrée de rendu direct.

La V2 ne fournit actuellement ni simulation de caméra/VFX, ni soumission de texte intégrée, ni fusion automatique des dessins, ni pooling des textures temporaires. Ces besoins restent à gérer dans l’application ou dans des passes adaptées.

Pour un exemple complet, voir [`crates/prism-v2/examples/v2.rs`](../crates/prism-v2/examples/v2.rs).
