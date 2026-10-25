use super::CatalogPayload;

/// Renders the complete, self-contained standalone HTML catalog document.
pub fn render_catalog_html(payload: &CatalogPayload) -> String {
    let json_data = serde_json::to_string(payload).unwrap_or_else(|_| "{}".to_string());
    let trip_name_escaped = html_escape(&payload.trip_name);
    let mode_display = match payload.mode.as_str() {
        "hierarchy" => "Hierarchy (APFS Hard Links)",
        _ => "In-Place Renamed",
    };

    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8"/>
  <meta name="viewport" content="width=device-width, initial-scale=1.0"/>
  <title>{trip_name} — Avicull Photo Catalog</title>
  <style>
    :root {{
      --bg-primary: #0b0f19;
      --bg-card: #151d30;
      --bg-card-hover: #1b2640;
      --border-color: #24324f;
      --border-focus: #3b82f6;
      --text-primary: #f8fafc;
      --text-secondary: #94a3b8;
      --text-muted: #64748b;
      --accent-green: #10b981;
      --accent-green-bg: rgba(16, 185, 129, 0.15);
      --accent-amber: #f59e0b;
      --accent-amber-bg: rgba(245, 158, 11, 0.15);
      --accent-slate: #64748b;
      --accent-slate-bg: rgba(100, 116, 139, 0.15);
      --accent-blue: #3b82f6;
      --accent-blue-hover: #2563eb;
    }}

    * {{
      box-sizing: border-box;
      margin: 0;
      padding: 0;
    }}

    body {{
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
      background-color: var(--bg-primary);
      color: var(--text-primary);
      line-height: 1.5;
      padding-bottom: 80px;
    }}

    header {{
      background: rgba(11, 15, 25, 0.85);
      backdrop-filter: blur(12px);
      -webkit-backdrop-filter: blur(12px);
      border-bottom: 1px solid var(--border-color);
      position: sticky;
      top: 0;
      z-index: 40;
      padding: 16px 24px;
    }}

    .header-content {{
      max-width: 1400px;
      margin: 0 auto;
      display: flex;
      justify-content: space-between;
      align-items: center;
      flex-wrap: wrap;
      gap: 16px;
    }}

    .brand-title {{
      display: flex;
      align-items: center;
      gap: 12px;
    }}

    .brand-title h1 {{
      font-size: 1.35rem;
      font-weight: 700;
      letter-spacing: -0.02em;
    }}

    .badge-mode {{
      font-size: 0.75rem;
      padding: 3px 8px;
      border-radius: 9999px;
      background: rgba(59, 130, 246, 0.15);
      color: #60a5fa;
      border: 1px solid rgba(59, 130, 246, 0.3);
      font-weight: 600;
      text-transform: uppercase;
      letter-spacing: 0.05em;
    }}

    .header-actions {{
      display: flex;
      align-items: center;
      gap: 12px;
    }}

    .btn {{
      display: inline-flex;
      align-items: center;
      gap: 8px;
      padding: 8px 16px;
      border-radius: 8px;
      font-size: 0.875rem;
      font-weight: 600;
      cursor: pointer;
      border: 1px solid transparent;
      transition: all 0.15s ease;
      text-decoration: none;
    }}

    .btn-primary {{
      background: var(--accent-blue);
      color: #ffffff;
    }}

    .btn-primary:hover {{
      background: var(--accent-blue-hover);
    }}

    .btn-secondary {{
      background: var(--bg-card);
      color: var(--text-primary);
      border-color: var(--border-color);
    }}

    .btn-secondary:hover {{
      background: var(--bg-card-hover);
      border-color: #3b82f6;
    }}

    .container {{
      max-width: 1400px;
      margin: 0 auto;
      padding: 24px;
    }}

    /* Summary Stats Grid */
    .stats-grid {{
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
      gap: 16px;
      margin-bottom: 24px;
    }}

    .stat-card {{
      background: var(--bg-card);
      border: 1px solid var(--border-color);
      border-radius: 12px;
      padding: 16px 20px;
      display: flex;
      flex-direction: column;
      gap: 4px;
    }}

    .stat-label {{
      font-size: 0.8rem;
      color: var(--text-secondary);
      text-transform: uppercase;
      letter-spacing: 0.05em;
      font-weight: 600;
    }}

    .stat-val {{
      font-size: 1.85rem;
      font-weight: 800;
      letter-spacing: -0.02em;
    }}

    .stat-confirmed .stat-val {{ color: var(--accent-green); }}
    .stat-review .stat-val {{ color: var(--accent-amber); }}
    .stat-species .stat-val {{ color: #38bdf8; }}

    /* Filters Bar */
    .controls-bar {{
      display: flex;
      flex-wrap: wrap;
      gap: 16px;
      align-items: center;
      justify-content: space-between;
      margin-bottom: 24px;
      background: var(--bg-card);
      border: 1px solid var(--border-color);
      border-radius: 12px;
      padding: 12px 16px;
    }}

    .search-box {{
      flex: 1;
      min-width: 260px;
      position: relative;
    }}

    .search-box input {{
      width: 100%;
      background: #0b0f19;
      border: 1px solid var(--border-color);
      border-radius: 8px;
      padding: 8px 12px 8px 36px;
      color: var(--text-primary);
      font-size: 0.875rem;
      outline: none;
      transition: border-color 0.15s ease;
    }}

    .search-box input:focus {{
      border-color: var(--border-focus);
    }}

    .search-icon {{
      position: absolute;
      left: 10px;
      top: 50%;
      transform: translateY(-50%);
      color: var(--text-muted);
      pointer-events: none;
    }}

    .filter-pills {{
      display: flex;
      gap: 8px;
      flex-wrap: wrap;
    }}

    .pill {{
      padding: 6px 14px;
      border-radius: 9999px;
      font-size: 0.8rem;
      font-weight: 600;
      cursor: pointer;
      background: #0b0f19;
      border: 1px solid var(--border-color);
      color: var(--text-secondary);
      transition: all 0.15s ease;
    }}

    .pill:hover, .pill.active {{
      background: var(--accent-blue);
      color: #ffffff;
      border-color: var(--accent-blue);
    }}

    /* Gallery Cards Grid */
    .gallery-grid {{
      display: grid;
      grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
      gap: 20px;
    }}

    .card {{
      background: var(--bg-card);
      border: 1px solid var(--border-color);
      border-radius: 12px;
      overflow: hidden;
      display: flex;
      flex-direction: column;
      transition: transform 0.15s ease, border-color 0.15s ease, box-shadow 0.15s ease;
    }}

    .card:hover {{
      transform: translateY(-2px);
      border-color: #3b82f6;
      box-shadow: 0 10px 25px -5px rgba(0, 0, 0, 0.4);
    }}

    .card-media {{
      position: relative;
      width: 100%;
      aspect-ratio: 4 / 3;
      background: #070a12;
      overflow: hidden;
      display: flex;
      align-items: center;
      justify-content: center;
    }}

    .card-media img {{
      width: 100%;
      height: 100%;
      object-fit: cover;
      display: block;
    }}

    .badge-tier {{
      position: absolute;
      top: 10px;
      right: 10px;
      font-size: 0.75rem;
      font-weight: 700;
      padding: 3px 8px;
      border-radius: 6px;
      backdrop-filter: blur(8px);
      -webkit-backdrop-filter: blur(8px);
    }}

    .badge-confirmed {{
      background: rgba(16, 185, 129, 0.9);
      color: #ffffff;
    }}

    .badge-review {{
      background: rgba(245, 158, 11, 0.9);
      color: #ffffff;
    }}

    .badge-nobird {{
      background: rgba(100, 116, 139, 0.9);
      color: #ffffff;
    }}

    .card-body {{
      padding: 16px;
      display: flex;
      flex-direction: column;
      gap: 8px;
      flex: 1;
    }}

    .card-species-title {{
      font-size: 1.1rem;
      font-weight: 700;
      color: var(--text-primary);
      display: flex;
      align-items: baseline;
      justify-content: space-between;
      gap: 8px;
    }}

    .card-scientific {{
      font-size: 0.85rem;
      font-style: italic;
      color: var(--text-secondary);
    }}

    .card-meta {{
      display: flex;
      flex-wrap: wrap;
      gap: 12px;
      font-size: 0.775rem;
      color: var(--text-muted);
      margin-top: 4px;
    }}

    .meta-item {{
      display: inline-flex;
      align-items: center;
      gap: 4px;
    }}

    .card-path {{
      font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
      font-size: 0.72rem;
      color: #94a3b8;
      background: #0b0f19;
      padding: 4px 8px;
      border-radius: 6px;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      margin-top: 6px;
      border: 1px solid rgba(255, 255, 255, 0.05);
    }}

    .card-actions {{
      margin-top: auto;
      padding-top: 12px;
      display: flex;
      gap: 8px;
    }}

    .btn-correct {{
      width: 100%;
      background: #1e293b;
      color: #cbd5e1;
      border: 1px solid var(--border-color);
      border-radius: 6px;
      padding: 6px 12px;
      font-size: 0.8rem;
      font-weight: 600;
      cursor: pointer;
      transition: all 0.15s ease;
      display: flex;
      align-items: center;
      justify-content: center;
      gap: 6px;
    }}

    .btn-correct:hover {{
      background: var(--accent-blue);
      color: #ffffff;
      border-color: var(--accent-blue);
    }}

    /* Cloud-Evicted / Fallback Tile */
    .preview-fallback {{
      width: 100%;
      height: 100%;
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      padding: 16px;
      text-align: center;
      background: radial-gradient(circle at center, #1e293b 0%, #0f172a 100%);
      color: var(--text-muted);
      gap: 8px;
    }}

    .preview-fallback-icon {{
      width: 36px;
      height: 36px;
      color: #64748b;
    }}

    .preview-fallback-text {{
      font-size: 0.8rem;
      font-weight: 600;
      color: #94a3b8;
    }}

    .preview-fallback-sub {{
      font-size: 0.7rem;
      color: #64748b;
    }}

    /* Modal Backdrop and Drawer */
    .modal-backdrop {{
      position: fixed;
      top: 0;
      left: 0;
      right: 0;
      bottom: 0;
      background: rgba(0, 0, 0, 0.75);
      backdrop-filter: blur(6px);
      -webkit-backdrop-filter: blur(6px);
      display: none;
      align-items: center;
      justify-content: center;
      z-index: 50;
      padding: 20px;
    }}

    .modal-backdrop.open {{
      display: flex;
    }}

    .modal-box {{
      background: var(--bg-card);
      border: 1px solid var(--border-color);
      border-radius: 16px;
      width: 100%;
      max-width: 650px;
      max-height: 90vh;
      display: flex;
      flex-direction: column;
      overflow: hidden;
      box-shadow: 0 25px 50px -12px rgba(0, 0, 0, 0.7);
    }}

    .modal-header {{
      padding: 18px 24px;
      border-bottom: 1px solid var(--border-color);
      display: flex;
      justify-content: space-between;
      align-items: center;
    }}

    .modal-header h2 {{
      font-size: 1.15rem;
      font-weight: 700;
    }}

    .modal-close {{
      background: transparent;
      border: none;
      color: var(--text-muted);
      cursor: pointer;
      font-size: 1.25rem;
      line-height: 1;
      padding: 4px;
    }}

    .modal-close:hover {{
      color: var(--text-primary);
    }}

    .modal-body {{
      padding: 24px;
      overflow-y: auto;
      display: flex;
      flex-direction: column;
      gap: 18px;
    }}

    .selected-photo-row {{
      display: flex;
      gap: 16px;
      align-items: center;
      background: #0b0f19;
      border: 1px solid var(--border-color);
      border-radius: 10px;
      padding: 12px;
    }}

    .modal-thumb {{
      width: 70px;
      height: 70px;
      border-radius: 8px;
      object-fit: cover;
      background: #1e293b;
      flex-shrink: 0;
    }}

    .modal-photo-details {{
      display: flex;
      flex-direction: column;
      gap: 4px;
      min-width: 0;
    }}

    .modal-photo-name {{
      font-size: 0.95rem;
      font-weight: 700;
      color: var(--text-primary);
    }}

    .modal-photo-sub {{
      font-size: 0.775rem;
      color: var(--text-secondary);
      font-family: ui-monospace, monospace;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }}

    .autocomplete-wrap {{
      position: relative;
    }}

    .autocomplete-input {{
      width: 100%;
      background: #0b0f19;
      border: 1px solid var(--border-color);
      border-radius: 8px;
      padding: 10px 14px;
      color: var(--text-primary);
      font-size: 0.95rem;
      outline: none;
    }}

    .autocomplete-input:focus {{
      border-color: var(--border-focus);
    }}

    .suggestions-list {{
      position: absolute;
      top: calc(100% + 6px);
      left: 0;
      right: 0;
      background: #0f172a;
      border: 1px solid var(--border-color);
      border-radius: 8px;
      max-height: 220px;
      overflow-y: auto;
      z-index: 60;
      display: none;
      box-shadow: 0 10px 25px rgba(0, 0, 0, 0.5);
    }}

    .suggestions-list.open {{
      display: block;
    }}

    .suggestion-item {{
      padding: 10px 14px;
      cursor: pointer;
      font-size: 0.875rem;
      color: var(--text-primary);
      border-bottom: 1px solid rgba(255, 255, 255, 0.04);
    }}

    .suggestion-item:hover, .suggestion-item.highlighted {{
      background: var(--accent-blue);
      color: #ffffff;
    }}

    .command-box {{
      background: #070a12;
      border: 1px solid var(--border-color);
      border-radius: 8px;
      padding: 14px;
      font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
      font-size: 0.8rem;
      color: #38bdf8;
      white-space: pre-wrap;
      word-break: break-all;
      line-height: 1.6;
    }}

    .modal-footer {{
      padding: 16px 24px;
      border-top: 1px solid var(--border-color);
      display: flex;
      justify-content: flex-end;
      gap: 12px;
    }}

    .empty-state {{
      grid-column: 1 / -1;
      text-align: center;
      padding: 60px 20px;
      color: var(--text-muted);
    }}
  </style>
</head>
<body>

<header>
  <div class="header-content">
    <div class="brand-title">
      <h1>{trip_name}</h1>
      <span class="badge-mode">{mode_display}</span>
    </div>
    <div class="header-actions">
      <button class="btn btn-secondary" id="btnDownloadBatch" onclick="downloadBatchScript()" title="Download bash script for all queued mislabel corrections">
        <svg width="16" height="16" fill="currentColor" viewBox="0 0 20 20"><path fill-rule="evenodd" d="M3 17a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1zm3.293-7.707a1 1 0 011.414 0L9 10.586V3a1 1 0 112 0v7.586l1.293-1.293a1 1 0 111.414 1.414l-3 3a1 1 0 01-1.414 0l-3-3a1 1 0 010-1.414z" clip-rule="evenodd"/></svg>
        Download apply_corrections.sh (<span id="batchCount">0</span>)
      </button>
    </div>
  </div>
</header>

<div class="container">
  <div class="stats-grid">
    <div class="stat-card">
      <span class="stat-label">Total Photos</span>
      <span class="stat-val" id="statTotal">{total_photos}</span>
    </div>
    <div class="stat-card stat-species">
      <span class="stat-label">Species Tally</span>
      <span class="stat-val" id="statSpecies">{species_count}</span>
    </div>
    <div class="stat-card stat-confirmed">
      <span class="stat-label">Confirmed Birds</span>
      <span class="stat-val" id="statConfirmed">{confirmed_count}</span>
    </div>
    <div class="stat-card stat-review">
      <span class="stat-label">Needs Review</span>
      <span class="stat-val" id="statReview">{needs_review_count}</span>
    </div>
  </div>

  <div class="controls-bar">
    <div class="search-box">
      <svg class="search-icon" width="16" height="16" fill="currentColor" viewBox="0 0 20 20"><path fill-rule="evenodd" d="M8 4a4 4 0 100 8 4 4 0 000-8zM2 8a6 6 0 1110.89 3.476l4.817 4.817a1 1 0 01-1.414 1.414l-4.816-4.816A6 6 0 012 8z" clip-rule="evenodd"/></svg>
      <input type="text" id="searchInput" placeholder="Search by species, location, date, or filename..." oninput="handleSearch()"/>
    </div>
    <div class="filter-pills">
      <button class="pill active" data-filter="all" onclick="setFilter('all', this)">All</button>
      <button class="pill" data-filter="ConfirmedSpecies" onclick="setFilter('ConfirmedSpecies', this)">Confirmed ({confirmed_count})</button>
      <button class="pill" data-filter="NeedsReview" onclick="setFilter('NeedsReview', this)">Needs Review ({needs_review_count})</button>
      <button class="pill" data-filter="NoBirdDetected" onclick="setFilter('NoBirdDetected', this)">No Bird ({no_bird_count})</button>
    </div>
  </div>

  <div class="gallery-grid" id="galleryGrid">
    <!-- Populated by JavaScript -->
  </div>
</div>

<!-- Mislabel Correction Modal -->
<div class="modal-backdrop" id="correctionModal">
  <div class="modal-box">
    <div class="modal-header">
      <h2>Correct Bird Species</h2>
      <button class="modal-close" onclick="closeModal()">&times;</button>
    </div>
    <div class="modal-body">
      <div class="selected-photo-row" id="modalPhotoSummary">
        <img id="modalPhotoThumb" class="modal-thumb" src="" alt="Thumbnail"/>
        <div class="modal-photo-details">
          <div class="modal-photo-name" id="modalCurrentSpecies">-</div>
          <div class="modal-photo-sub" id="modalCurrentPath">-</div>
        </div>
      </div>

      <div>
        <label style="font-size: 0.85rem; font-weight: 600; color: var(--text-secondary); margin-bottom: 6px; display: block;">
          Select Correct Species (eBird Taxonomy Autocomplete):
        </label>
        <div class="autocomplete-wrap">
          <input type="text" id="speciesSearchInput" class="autocomplete-input" placeholder="Start typing bird name (e.g. Painted Stork)..." autocomplete="off" oninput="handleSpeciesAutocomplete()"/>
          <div class="suggestions-list" id="speciesSuggestions"></div>
        </div>
      </div>

      <div>
        <label style="font-size: 0.85rem; font-weight: 600; color: var(--text-secondary); margin-bottom: 6px; display: block;">
          Generated Unix Shell Command (mv):
        </label>
        <div style="font-size: 0.78rem; color: var(--text-muted); margin-bottom: 6px;">
          Note: Run from the photo catalog directory or execute the downloadable script below.
        </div>
        <div class="command-box" id="commandOutput">Select a species above to compute destination filename and mv command...</div>
      </div>
    </div>
    <div class="modal-footer">
      <button class="btn btn-secondary" id="btnCopyCommand" onclick="copyCurrentCommand()">Copy Command</button>
      <button class="btn btn-primary" id="btnAddToBatch" onclick="addToBatch()">Add to Batch Script</button>
    </div>
  </div>
</div>

<!-- Embedded JSON Data for Offline Interactivity -->
<script id="catalog-data" type="application/json">
{json_data}
</script>

<script>
  let CATALOG_DATA = {{}};
  try {{
    const el = document.getElementById('catalog-data');
    if (el) {{
      CATALOG_DATA = JSON.parse(el.textContent);
    }}
  }} catch (e) {{
    console.error("Failed to parse catalog-data:", e);
  }}

  const items = CATALOG_DATA.items || [];
  const speciesList = CATALOG_DATA.species_list || [];
  let currentFilter = 'all';
  let searchQuery = '';
  let activeModalItem = null;
  let batchCommands = [];

  function sanitizeSlug(name) {{
    if (!name) return "";
    return name
      .replace(/'/g, '')
      .replace(/\s+/g, '_')
      .replace(/[^a-zA-Z0-9_-]/g, '_')
      .replace(/_+/g, '_')
      .replace(/^_+|_+$/g, '');
  }}

  function handleImageError(img, itemId) {{
    const parent = img.parentElement;
    if (!parent) return;
    const item = items.find(i => i.id === itemId);
    const filename = item ? item.primary_filename : "File";
    parent.innerHTML = `
      <div class="preview-fallback">
        <svg class="preview-fallback-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M3 15a4 4 0 004 4h9a5 5 0 10-.1-9.999 5.002 5.002 0 00-9.78 2.096A4.001 4.001 0 003 15z"></path>
        </svg>
        <div class="preview-fallback-text">${{filename}}</div>
        <div class="preview-fallback-sub">Preview Unavailable (Cloud Evicted or Unsupported RAW)</div>
      </div>
    `;
  }}

  function renderGallery() {{
    const grid = document.getElementById('galleryGrid');
    if (!grid) return;

    const filtered = items.filter(item => {{
      const matchFilter = currentFilter === 'all' || item.routing_tier === currentFilter;
      if (!matchFilter) return false;
      if (!searchQuery) return true;

      const q = searchQuery.toLowerCase();
      const sp = (item.assigned_species || '').toLowerCase();
      const sc = (item.scientific_name || '').toLowerCase();
      const loc = (item.location_name || '').toLowerCase();
      const dt = (item.capture_date || '').toLowerCase();
      const fn = (item.primary_filename || '').toLowerCase();
      return sp.includes(q) || sc.includes(q) || loc.includes(q) || dt.includes(q) || fn.includes(q);
    }});

    if (filtered.length === 0) {{
      grid.innerHTML = `<div class="empty-state"><h3>No photos matched your filter or search query.</h3></div>`;
      return;
    }}

    grid.innerHTML = filtered.map(item => {{
      const confPct = Math.round(item.confidence * 100);
      let tierBadgeClass = "badge-confirmed";
      let tierLabel = `${{confPct}}%`;

      if (item.routing_tier === 'NeedsReview') {{
        tierBadgeClass = "badge-review";
        tierLabel = `Review (${{confPct}}%)`;
      }} else if (item.routing_tier === 'NoBirdDetected') {{
        tierBadgeClass = "badge-nobird";
        tierLabel = "No Bird";
      }}

      const locationMarkup = item.location_name ? `
        <span class="meta-item">
          <svg width="12" height="12" fill="currentColor" viewBox="0 0 20 20"><path fill-rule="evenodd" d="M5.05 4.05a7 7 0 119.9 9.9L10 18.9l-4.95-4.95a7 7 0 010-9.9zM10 11a2 2 0 100-4 2 2 0 000 4z" clip-rule="evenodd"/></svg>
          ${{item.location_name}}
        </span>
      ` : '';

      const dateMarkup = item.capture_date ? `
        <span class="meta-item">
          <svg width="12" height="12" fill="currentColor" viewBox="0 0 20 20"><path fill-rule="evenodd" d="M6 2a1 1 0 00-1 1v1H4a2 2 0 00-2 2v10a2 2 0 002 2h12a2 2 0 002-2V6a2 2 0 00-2-2h-1V3a1 1 0 10-2 0v1H7V3a1 1 0 00-1-1zm0 5a1 1 0 000 2h8a1 1 0 100-2H6z" clip-rule="evenodd"/></svg>
          ${{item.capture_date}}
        </span>
      ` : '';

      const scientificMarkup = item.scientific_name ? `
        <div class="card-scientific">${{item.scientific_name}}</div>
      ` : '';

      return `
        <div class="card" id="card-${{item.id}}">
          <div class="card-media">
            <img src="${{item.preview_url}}" alt="${{item.assigned_species}}" loading="lazy" onerror="handleImageError(this, '${{item.id}}')"/>
            <span class="badge-tier ${{tierBadgeClass}}">${{tierLabel}}</span>
          </div>
          <div class="card-body">
            <div class="card-species-title">
              <span>${{item.assigned_species}}</span>
            </div>
            ${{scientificMarkup}}
            <div class="card-meta">
              ${{locationMarkup}}
              ${{dateMarkup}}
            </div>
            <div class="card-path" title="${{item.current_path}}">${{item.current_path}}</div>
            <div class="card-actions">
              <button class="btn-correct" onclick="openModal('${{item.id}}')">
                <svg width="14" height="14" fill="currentColor" viewBox="0 0 20 20"><path d="M13.586 3.586a2 2 0 112.828 2.828l-.793.793-2.828-2.828.793-.793zM11.379 5.793L3 14.172V17h2.828l8.38-8.379-2.83-2.828z"/></svg>
                Correct Species
              </button>
            </div>
          </div>
        </div>
      `;
    }}).join('');
  }}

  function setFilter(tier, btn) {{
    currentFilter = tier;
    document.querySelectorAll('.filter-pills .pill').forEach(p => p.classList.remove('active'));
    if (btn) btn.classList.add('active');
    renderGallery();
  }}

  function handleSearch() {{
    const el = document.getElementById('searchInput');
    searchQuery = el ? el.value.trim() : '';
    renderGallery();
  }}

  function openModal(itemId) {{
    activeModalItem = items.find(i => i.id === itemId);
    if (!activeModalItem) return;

    document.getElementById('modalCurrentSpecies').textContent = activeModalItem.assigned_species;
    document.getElementById('modalCurrentPath').textContent = activeModalItem.current_path;
    const thumb = document.getElementById('modalPhotoThumb');
    if (thumb) {{
      thumb.src = activeModalItem.preview_url;
      thumb.onerror = () => {{ thumb.style.display = 'none'; }};
      thumb.style.display = 'block';
    }}

    const input = document.getElementById('speciesSearchInput');
    if (input) input.value = '';
    document.getElementById('speciesSuggestions').classList.remove('open');
    document.getElementById('commandOutput').textContent = "Select a species from the autocomplete search above to generate Unix mv commands...";

    document.getElementById('correctionModal').classList.add('open');
  }}

  function closeModal() {{
    document.getElementById('correctionModal').classList.remove('open');
    activeModalItem = null;
  }}

  function handleSpeciesAutocomplete() {{
    const input = document.getElementById('speciesSearchInput');
    const suggestionsEl = document.getElementById('speciesSuggestions');
    if (!input || !suggestionsEl) return;

    const val = input.value.trim().toLowerCase();
    if (!val || val.length < 2) {{
      suggestionsEl.classList.remove('open');
      return;
    }}

    const matches = speciesList.filter(s => s.toLowerCase().includes(val)).slice(0, 15);
    if (matches.length === 0) {{
      suggestionsEl.classList.remove('open');
      return;
    }}

    suggestionsEl.innerHTML = matches.map(s => `
      <div class="suggestion-item" onclick="selectSpecies('${{s.replace(/'/g, "\\'")}}')">${{s}}</div>
    `).join('');
    suggestionsEl.classList.add('open');
  }}

  function selectSpecies(species) {{
    const input = document.getElementById('speciesSearchInput');
    if (input) input.value = species;
    document.getElementById('speciesSuggestions').classList.remove('open');

    if (!activeModalItem) return;
    const isHierarchy = CATALOG_DATA.mode === 'hierarchy';
    const newSlug = sanitizeSlug(species);

    let commands = [];
    if (CATALOG_DATA.catalog_dir) {{
      commands.push(`cd "${{CATALOG_DATA.catalog_dir}}"`);
    }}
    const currentPath = activeModalItem.current_path;
    const lastSlash = currentPath.lastIndexOf('/');
    const dir = lastSlash >= 0 ? currentPath.substring(0, lastSlash) : '';
    const allSidecars = [...(activeModalItem.sidecar_files || []), ...(activeModalItem.companion_files || [])];

    if (isHierarchy) {{
      const targetDir = `birds_hierarchy/${{newSlug}}`;
      commands.push(`mkdir -p "${{targetDir}}"`);
      const filename = lastSlash >= 0 ? currentPath.substring(lastSlash + 1) : currentPath;
      commands.push(`mv "${{currentPath}}" "${{targetDir}}/${{filename}}"`);

      for (const sc of allSidecars) {{
        const scSlash = sc.lastIndexOf('/');
        const scFilename = scSlash >= 0 ? sc.substring(scSlash + 1) : sc;
        commands.push(`mv "${{sc}}" "${{targetDir}}/${{scFilename}}"`);
      }}
    }} else {{
      // In-place rename format: <stem>_<Species>[_<Date>][_<Location>].<ext>
      const originalStem = activeModalItem.original_stem;
      const datePart = activeModalItem.capture_date ? activeModalItem.capture_date.replace(/[-:/]/g, '') : '';
      const locPart = activeModalItem.location_name ? sanitizeSlug(activeModalItem.location_name) : '';

      let tags = [];
      if (newSlug) tags.push(newSlug);
      if (datePart && datePart.toLowerCase() !== 'unknown') tags.push(datePart);
      if (locPart && locPart.toLowerCase() !== 'unknown') tags.push(locPart);

      const targetStem = tags.length > 0 ? `${{originalStem}}_${{tags.join('_')}}` : originalStem;

      // Primary file extension
      const primaryExt = getExtension(activeModalItem.primary_filename);
      const newPrimary = (dir ? dir + '/' : '') + targetStem + primaryExt;
      commands.push(`mv "${{currentPath}}" "${{newPrimary}}"`);

      // Companion and sidecar files
      for (const sc of allSidecars) {{
        const scSlash = sc.lastIndexOf('/');
        const scDir = scSlash >= 0 ? sc.substring(0, scSlash) : dir;
        const scFilename = scSlash >= 0 ? sc.substring(scSlash + 1) : sc;
        const scExt = getCompoundExtension(scFilename);
        const newSc = (scDir ? scDir + '/' : '') + targetStem + scExt;
        commands.push(`mv "${{sc}}" "${{newSc}}"`);
      }}
    }}

    const cmdStr = commands.join('\n');
    document.getElementById('commandOutput').textContent = cmdStr;
    activeModalItem._pendingCommands = commands;
  }}

  function getExtension(filename) {{
    const idx = filename.indexOf('.');
    return idx >= 0 ? filename.substring(idx) : '';
  }}

  function getCompoundExtension(filename) {{
    const m = filename.match(/(\.[a-zA-Z0-9]+(?:\.xmp)?)$/i);
    return m ? m[1] : '';
  }}

  function copyCurrentCommand() {{
    const text = document.getElementById('commandOutput').textContent;
    if (!text || text.startsWith("Select a species")) return;

    navigator.clipboard.writeText(text).then(() => {{
      const btn = document.getElementById('btnCopyCommand');
      const orig = btn.textContent;
      btn.textContent = "Copied!";
      setTimeout(() => {{ btn.textContent = orig; }}, 1500);
    }}).catch(() => {{
      alert("Command: \n\n" + text);
    }});
  }}

  function addToBatch() {{
    if (!activeModalItem || !activeModalItem._pendingCommands) return;
    batchCommands.push(...activeModalItem._pendingCommands);
    document.getElementById('batchCount').textContent = batchCommands.length;
    closeModal();
  }}

  function downloadBatchScript() {{
    if (batchCommands.length === 0) {{
      alert("No mislabel corrections have been added to the batch yet. Click 'Correct Species' on any photo and select 'Add to Batch Script'.");
      return;
    }}

    const scriptContent = [
      "#!/usr/bin/env bash",
      "# Avicull Mislabel Corrections Batch Script",
      "# Generated automatically by avicull standalone catalog",
      "set -euo pipefail",
      "",
      `cd "${{CATALOG_DATA.catalog_dir || '.'}}"`,
      "",
      ...batchCommands,
      "",
      "echo '✓ All avicull mislabel corrections applied successfully!'",
      ""
    ].join('\n');

    const blob = new Blob([scriptContent], {{ type: 'text/x-sh' }});
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'apply_corrections.sh';
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  }}

  // Initialize on load
  document.addEventListener('DOMContentLoaded', () => {{
    renderGallery();
  }});
</script>

</body>
</html>
"##,
        trip_name = trip_name_escaped,
        mode_display = mode_display,
        total_photos = payload.stats.total_photos,
        species_count = payload.stats.species_count,
        confirmed_count = payload.stats.confirmed_count,
        needs_review_count = payload.stats.needs_review_count,
        no_bird_count = payload.stats.no_bird_count,
        json_data = json_data
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
