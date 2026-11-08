# Concepts

> Shared domain vocabulary for this project — entities, named processes, and status concepts with project-specific meaning. Seeded with core domain vocabulary, then accretes as ce-compound captures and refreshes learnings; direct edits are fine. Glossary only, not a spec or catch-all.

## Storage & Filesystem Architecture

### AssetBundle
The atomic unit of management representing a primary photographic image together with all its associated metadata and editing sidecars.
*Avoid:* Photo, image file, raw file

An AssetBundle binds a raw image or JPEG payload to its sidecars so that downstream operations never separate an image from its capture metadata or edit history. All bundling, hashing, and link operations treat the bundle as an indivisible entity.

### SidecarPair
The association between a primary image file and an auxiliary data file carrying external adjustments, color ratings, or metadata tags.
*Avoid:* Metadata file, companion file

Sidecars share the same base stem as their primary image or append an auxiliary extension. Deleting, moving, or linking an AssetBundle must atomically propagate across all files in the pair.

### HardLinkTree
An organized directory hierarchy referencing photographic assets on the same local volume through filesystem inodes, consuming zero additional disk space.
*Avoid:* Copy tree, export folder

A HardLinkTree creates multiple organized views of an archive without duplicating image payloads. Because hard links share underlying storage, modifying an image payload in one tree alters it in all linked trees, but creating, moving, or unlinking path entries is strictly safe and isolated.

### VirtualMount
An on-demand virtual filesystem mount providing a local path interface to stream and inspect remote or cloud-hosted photographic archives.
*Avoid:* Cloud sync, local cache

A VirtualMount allows directory scanning and thumbnail extraction to execute over network channels without requiring an eager, full download of the entire remote archive.

### DatalessFaultGuard
A filesystem inspection mechanism that detects cloud-managed placeholder files and prevents initiating blocking network downloads during archive traversal.
*Avoid:* Cloud check, hydration guard

Dataless files appear as normal directory entries but lack local disk blocks. Reading or opening their content triggers an operating system cloud fault that blocks until downloaded; the guard inspects file flags beforehand to skip or defer unhydrated files.

## Taxonomy & Classification

### TaxonIdentifier
An immutable, permanent alphanumeric key designating a biological taxon within standardized global biodiversity taxonomies.
*Avoid:* Species code, scientific name

TaxonIdentifiers remain constant across taxonomy revisions, even when ornithological committees split, lump, or alter scientific and common species names.

### FolderSlug
A filesystem-sanitized directory name incorporating common species or subject names, capture dates, and inferred regional tags.
*Avoid:* Category name, folder title

A FolderSlug is derived from taxonomy and metadata for human readability in directory navigators. Unlike the underlying TaxonIdentifier, a FolderSlug can change when common names or regional aliases are updated.

### AssetCategory
The primary domain classification dividing assets into broad visual and biodiversity kingdoms prior to fine-grained classification.
*Avoid:* Classification type, asset type

Categories partition the archive into distinct branches such as birds, other wildlife, botanical subjects, landscapes, portraits, architecture, or non-subject images. This partitioning prevents bird classification models from forcing non-avian photos into avian species categories.

### SpatialPrior
A probability distribution over geographic taxa derived from capture coordinates or inferred waypoint proximity, weighting species likelihoods against visual evidence.
*Avoid:* Location filter, regional heuristic

A SpatialPrior scales neural classification confidence based on historical regional occurrence data. Species impossible for a geographical region are down-weighted without completely overriding overwhelming visual confidence.

### BurstConsensus
A consensus aggregation mechanism that unifies classification across a series of images captured in rapid temporal succession.
*Avoid:* Group voting, burst clustering

Burst photos of the same subject under fluctuating lighting or angles may yield divergent single-frame predictions; consensus pooling aggregates visual signals across the burst to assign a single authoritative taxon.

### GenreTaxonomy
The classification schema governing non-wildlife and photographic genre subjects.
*Avoid:* Scene classification, custom categories

GenreTaxonomy provides standardized targets for non-biological photos (e.g. Scenery, Botanical, Architecture, Portrait, Abstract) so that mixed archives maintain consistent organization without false biological assignments.

## Manifest & State Management

### ManifestStore
An embedded, portable database recording asset identities, cryptographic content digests, classification embeddings, and human overrides.
*Avoid:* Cache, metadata index

The ManifestStore acts as the persistent ledger for an archive. It survives re-runs and directory relocations, ensuring that expensive neural classifications and manual human corrections are preserved permanently.

### UserCorrection
An explicit human classification or location override submitted to correct automated neural inferences.
*Avoid:* Manual tag, user edit

A UserCorrection takes absolute precedence over neural predictions and spatial priors in all subsequent catalog generations and filesystem reorganizations.

### DeltaScan
An incremental traversal pass that consults previous manifest records to bypass neural inference and metadata extraction for unmodified assets.
*Avoid:* Quick scan, incremental run

DeltaScans compare filesystem timestamps and file identity against existing manifest records, executing expensive neural inference only on newly introduced or modified files.

### CatalogView
A standalone, zero-dependency interactive interface generated within the organized hierarchy for visual verification and correction submission.
*Avoid:* Web gallery, preview page

The CatalogView runs entirely client-side without a running backend server, loading thumbnail streams, embedding canonical taxonomy definitions for offline category routing, and allowing reviewers to submit corrections directly back into the local manifest.

## Flagged ambiguities

- 'TaxonIdentifier' had been used interchangeably with 'FolderSlug' — these are distinct: the identifier is an immutable upstream key, while the slug is a human-readable, mutable folder name.
- 'AssetBundle' had been confused with single image files — an AssetBundle is an atomic group encompassing both the primary image and all its associated sidecars.
- 'NoSubjectDetected' had been conflated with unclassified birds — it is a distinct top-level category signifying that no identifiable focal subject exists in the frame.
