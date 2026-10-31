import { test, expect } from '@playwright/test';
import path from 'path';

const FIXTURE_PATH = `file://${path.resolve(__dirname, '../../fixtures/test_bird_catalog.html')}`;

// Helper modifier: Mac uses 'Meta', other OSes can use 'Control' or 'Meta'
const CMD_MODIFIER = process.platform === 'darwin' ? 'Meta' : 'Control';

test.describe('Avicull Bird Photo Catalog E2E Tests', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(FIXTURE_PATH);
    await page.waitForLoadState('domcontentloaded');
  });

  test.describe('Initial Render & Statistics', () => {
    test('renders header title, mode badge, and batch action buttons with zero count', async ({ page }) => {
      await expect(page.locator('header h1')).toHaveText('Himalayan & Plains Birding 2026');
      await expect(page.locator('header .badge-mode')).toHaveText('In-Place Renamed');

      const batchCount = page.locator('#batchCount');
      await expect(batchCount).toHaveText('0');
      const batchCountCopy = page.locator('#batchCountCopy');
      await expect(batchCountCopy).toHaveText('0');

      await expect(page.locator('#btnDownloadBatch')).toContainText('Download apply_corrections.sh (0)');
      await expect(page.locator('#btnCopyBatchScript')).toContainText('Copy Batch Script (0)');
    });

    test('displays accurate summary stat cards', async ({ page }) => {
      await expect(page.locator('#statTotal')).toHaveText('8');
      await expect(page.locator('#statSpecies')).toHaveText('6');
      await expect(page.locator('#statConfirmed')).toHaveText('4');
      await expect(page.locator('#statReview')).toHaveText('2');
    });

    test('renders all 8 cards initially and keeps selection bar / modal hidden', async ({ page }) => {
      const cards = page.locator('#galleryGrid .card');
      await expect(cards).toHaveCount(8);

      const selectionBar = page.locator('#selectionBar');
      await expect(selectionBar).not.toHaveClass(/visible/);

      const modal = page.locator('#correctionModal');
      await expect(modal).not.toHaveClass(/open/);
    });
  });

  test.describe('Filter Pills Navigation', () => {
    test('filters to Confirmed cards only', async ({ page }) => {
      const confirmedPill = page.locator('.filter-pills button[data-filter="ConfirmedSpecies"]');
      await confirmedPill.click();
      await expect(confirmedPill).toHaveClass(/active/);

      const cards = page.locator('#galleryGrid .card');
      await expect(cards).toHaveCount(4);
      for (let i = 0; i < 4; i++) {
        await expect(cards.nth(i).locator('.badge-tier')).toHaveClass(/badge-confirmed/);
      }
    });

    test('filters to Needs Review cards only', async ({ page }) => {
      const reviewPill = page.locator('.filter-pills button[data-filter="NeedsReview"]');
      await reviewPill.click();
      await expect(reviewPill).toHaveClass(/active/);

      const cards = page.locator('#galleryGrid .card');
      await expect(cards).toHaveCount(2);
      for (let i = 0; i < 2; i++) {
        await expect(cards.nth(i).locator('.badge-tier')).toHaveClass(/badge-review/);
      }
    });

    test('filters to No Bird cards only', async ({ page }) => {
      const noBirdPill = page.locator('.filter-pills button[data-filter="NoBirdDetected"]');
      await noBirdPill.click();
      await expect(noBirdPill).toHaveClass(/active/);

      const cards = page.locator('#galleryGrid .card');
      await expect(cards).toHaveCount(2);
      for (let i = 0; i < 2; i++) {
        await expect(cards.nth(i).locator('.badge-tier')).toHaveClass(/badge-nobird/);
      }
    });

    test('resets filter back to All', async ({ page }) => {
      // First switch to No Bird
      await page.locator('.filter-pills button[data-filter="NoBirdDetected"]').click();
      await expect(page.locator('#galleryGrid .card')).toHaveCount(2);

      // Now click All
      const allPill = page.locator('.filter-pills button[data-filter="all"]');
      await allPill.click();
      await expect(allPill).toHaveClass(/active/);
      await expect(page.locator('#galleryGrid .card')).toHaveCount(8);
    });
  });

  test.describe('Search Input Filtering', () => {
    test('filters cards dynamically by species common name', async ({ page }) => {
      const searchInput = page.locator('#searchInput');
      await searchInput.fill('Sparrow');

      const cards = page.locator('#galleryGrid .card');
      await expect(cards).toHaveCount(2);
      await expect(cards.nth(0)).toContainText('House Sparrow');
      await expect(cards.nth(1)).toContainText('Eurasian Tree Sparrow');
    });

    test('filters cards dynamically by primary filename', async ({ page }) => {
      const searchInput = page.locator('#searchInput');
      await searchInput.fill('IMG_1003');

      const cards = page.locator('#galleryGrid .card');
      await expect(cards).toHaveCount(1);
      await expect(cards.first()).toContainText('Coppersmith Barbet');
    });

    test('displays empty state message when search yields no matches', async ({ page }) => {
      const searchInput = page.locator('#searchInput');
      await searchInput.fill('NonExistentPenguinXYZ');

      const cards = page.locator('#galleryGrid .card');
      await expect(cards).toHaveCount(0);
      const emptyState = page.locator('.empty-state');
      await expect(emptyState).toBeVisible();
      await expect(emptyState).toContainText('No photos matched your filter or search query.');

      // Clearing search restores items
      await searchInput.fill('');
      await expect(page.locator('#galleryGrid .card')).toHaveCount(8);
    });
  });

  test.describe('Card Selection & Multi-Select Flows', () => {
    test('single click selects 1 card and displays floating selection bar', async ({ page }) => {
      const firstCard = page.locator('#card-bundle-1');
      await firstCard.click();

      await expect(firstCard).toHaveClass(/selected/);
      const bar = page.locator('#selectionBar');
      await expect(bar).toHaveClass(/visible/);
      await expect(page.locator('#selectedCountBadge')).toHaveText('1');
      await expect(page.locator('#selectedCountBtn')).toHaveText('1');
    });

    test('Cmd/Ctrl+Click enables disjoint multi-selection with updated count', async ({ page }) => {
      const firstCard = page.locator('#card-bundle-1');
      const secondCard = page.locator('#card-bundle-2');

      // Click first card
      await firstCard.click();
      await expect(firstCard).toHaveClass(/selected/);

      // Cmd/Ctrl+Click second card
      await secondCard.click({ modifiers: [CMD_MODIFIER] });

      await expect(firstCard).toHaveClass(/selected/);
      await expect(secondCard).toHaveClass(/selected/);
      await expect(page.locator('#selectedCountBadge')).toHaveText('2');
      await expect(page.locator('#selectedCountBtn')).toHaveText('2');
    });

    test('Shift+Click selects contiguous range of cards and updates count', async ({ page }) => {
      // Click 1st card (bundle-1)
      await page.locator('#card-bundle-1').click();
      await expect(page.locator('#selectedCountBadge')).toHaveText('1');

      // Shift+Click 4th card (bundle-4) -> should select bundle-1, bundle-2, bundle-3, bundle-4
      await page.locator('#card-bundle-4').click({ modifiers: ['Shift'] });

      await expect(page.locator('#card-bundle-1')).toHaveClass(/selected/);
      await expect(page.locator('#card-bundle-2')).toHaveClass(/selected/);
      await expect(page.locator('#card-bundle-3')).toHaveClass(/selected/);
      await expect(page.locator('#card-bundle-4')).toHaveClass(/selected/);

      await expect(page.locator('#selectedCountBadge')).toHaveText('4');
      await expect(page.locator('#selectedCountBtn')).toHaveText('4');
    });

    test('Clear selection button clears all selected cards and hides floating bar', async ({ page }) => {
      // Select two cards
      await page.locator('#card-bundle-1').click();
      await page.locator('#card-bundle-2').click({ modifiers: [CMD_MODIFIER] });
      await expect(page.locator('#selectionBar')).toHaveClass(/visible/);

      // Click Clear button in floating bar
      await page.locator('#selectionBar button:has-text("Clear")').click();

      await expect(page.locator('#selectionBar')).not.toHaveClass(/visible/);
      await expect(page.locator('#card-bundle-1')).not.toHaveClass(/selected/);
      await expect(page.locator('#card-bundle-2')).not.toHaveClass(/selected/);
    });

    test('Escape key clears selection and hides floating bar', async ({ page }) => {
      await page.locator('#card-bundle-1').click();
      await expect(page.locator('#selectionBar')).toHaveClass(/visible/);

      // Press Escape key
      await page.keyboard.press('Escape');

      await expect(page.locator('#selectionBar')).not.toHaveClass(/visible/);
      await expect(page.locator('#card-bundle-1')).not.toHaveClass(/selected/);
    });

    test('Cmd+A / Ctrl+A selects all visible cards', async ({ page }) => {
      // Focus outside inputs (e.g. click body or container)
      await page.locator('.container').click();

      // Press Cmd+A or Ctrl+A
      await page.keyboard.press(`${CMD_MODIFIER}+a`);

      const cards = page.locator('#galleryGrid .card');
      const count = await cards.count();
      expect(count).toBe(8);

      for (let i = 0; i < count; i++) {
        await expect(cards.nth(i)).toHaveClass(/selected/);
      }

      await expect(page.locator('#selectedCountBadge')).toHaveText('8');
      await expect(page.locator('#selectionBar')).toHaveClass(/visible/);

      // Press Escape to clear
      await page.keyboard.press('Escape');
      await expect(page.locator('#selectionBar')).not.toHaveClass(/visible/);
    });
  });

  test.describe('Species Correction Modal', () => {
    test('opens modal from card "Correct Species" button for single card', async ({ page }) => {
      const correctBtn = page.locator('#card-bundle-1 .btn-correct');
      await correctBtn.click();

      const modal = page.locator('#correctionModal');
      await expect(modal).toHaveClass(/open/);

      await expect(page.locator('#modalCurrentSpecies')).toHaveText('House Sparrow');
      await expect(page.locator('#modalCurrentPath')).toContainText('IMG_1001_House_Sparrow');
    });

    test('autocomplete search suggests matches and clicking suggestion updates command preview', async ({ page }) => {
      // Open modal for card 1
      await page.locator('#card-bundle-1 .btn-correct').click();
      await expect(page.locator('#correctionModal')).toHaveClass(/open/);

      // Type "Barbet" in autocomplete input
      const speciesInput = page.locator('#speciesSearchInput');
      await speciesInput.fill('Barbet');

      const suggestionsList = page.locator('#speciesSuggestions');
      await expect(suggestionsList).toHaveClass(/open/);

      // Should suggest Coppersmith Barbet and Great Barbet
      const suggestionItems = suggestionsList.locator('.suggestion-item');
      await expect(suggestionItems).toHaveCount(2);
      await expect(suggestionItems.nth(0)).toHaveText('Coppersmith Barbet');
      await expect(suggestionItems.nth(1)).toHaveText('Great Barbet');

      // Click "Great Barbet"
      await suggestionItems.nth(1).click();

      await expect(speciesInput).toHaveValue('Great Barbet');
      await expect(suggestionsList).not.toHaveClass(/open/);

      // Command output preview should update with Great_Barbet
      const commandBox = page.locator('#commandOutput');
      await expect(commandBox).toContainText('Great_Barbet');
      await expect(commandBox).toContainText('IMG_1001');
    });

    test('"Mark as No Bird Detected" button sets species and updates command preview', async ({ page }) => {
      // Open modal for card 1
      await page.locator('#card-bundle-1 .btn-correct').click();
      await expect(page.locator('#correctionModal')).toHaveClass(/open/);

      // Click "Mark as No Bird Detected" button
      await page.locator('#btnMarkNoBird').click();

      await expect(page.locator('#speciesSearchInput')).toHaveValue('No Bird Detected');
      const commandBox = page.locator('#commandOutput');
      await expect(commandBox).toContainText('_No_Bird_Detected');
    });

    test('"Add to Batch Script" adds correction to batch, closes modal, and updates header counter', async ({ page }) => {
      // Ensure initial counter is 0
      await expect(page.locator('#batchCount')).toHaveText('0');

      // Open modal for card 1
      await page.locator('#card-bundle-1 .btn-correct').click();
      await expect(page.locator('#correctionModal')).toHaveClass(/open/);

      // Select a new species via autocomplete
      await page.locator('#speciesSearchInput').fill('Painted Stork');
      await page.locator('#speciesSuggestions .suggestion-item:has-text("Painted Stork")').click();

      // Click "Add to Batch Script"
      await page.locator('#btnAddToBatch').click();

      // Modal should be closed
      await expect(page.locator('#correctionModal')).not.toHaveClass(/open/);

      // Header batch counts should update to 1
      await expect(page.locator('#batchCount')).toHaveText('1');
      await expect(page.locator('#batchCountCopy')).toHaveText('1');
      await expect(page.locator('#btnDownloadBatch')).toContainText('Download apply_corrections.sh (1)');
    });

    test('multi-item modal displays selected count, stem summaries, and batch adds all', async ({ page }) => {
      // Select cards 1 and 2
      await page.locator('#card-bundle-1').click();
      await page.locator('#card-bundle-2').click({ modifiers: [CMD_MODIFIER] });

      await expect(page.locator('#selectedCountBadge')).toHaveText('2');

      // Click floating bar "Correct Selected (2)"
      await page.locator('#btnCorrectSelected').click();

      const modal = page.locator('#correctionModal');
      await expect(modal).toHaveClass(/open/);

      // Summary displays "2 Photos Selected" and stem list
      await expect(page.locator('#modalCurrentSpecies')).toHaveText('2 Photos Selected');
      await expect(page.locator('#modalCurrentPath')).toContainText('IMG_1001, IMG_1002');

      // Type "Sparrow" in autocomplete
      const speciesInput = page.locator('#speciesSearchInput');
      await speciesInput.fill('Sparrow');

      const suggestions = page.locator('#speciesSuggestions .suggestion-item');
      await expect(suggestions).toHaveCount(2);

      // Choose "House Sparrow"
      await suggestions.filter({ hasText: 'House Sparrow' }).first().click();

      // Command output preview contains instructions for both items
      const cmdBox = page.locator('#commandOutput');
      await expect(cmdBox).toContainText('IMG_1001');
      await expect(cmdBox).toContainText('IMG_1002');

      // Add to batch
      await page.locator('#btnAddToBatch').click();
      await expect(modal).not.toHaveClass(/open/);

      // Header count should increment by 2
      await expect(page.locator('#batchCount')).toHaveText('2');
      await expect(page.locator('#btnDownloadBatch')).toContainText('Download apply_corrections.sh (2)');
    });
  });

  test.describe('Trip Location Override Modal', () => {
    test('displays current trip location in header and opens location modal', async ({ page }) => {
      // Verify explicit location label
      const prefix = page.locator('.location-prefix');
      await expect(prefix).toBeVisible();
      await expect(prefix).toHaveText('Location of the current directory:');

      const locBadge = page.locator('#headerLocationBadge');
      await expect(locBadge).toBeVisible();
      await expect(page.locator('#headerLocationText')).toContainText('Leh, Ladakh');

      // Verify dedicated "modify locations of directory and subdirectory" button to the right
      const modifyBtn = page.locator('#btnModifyLocations');
      await expect(modifyBtn).toBeVisible();
      await expect(modifyBtn).toHaveText('modify locations of directory and subdirectory');

      // Verify dedicated "Review your classifications" button in header
      const mainReviewBtn = page.locator('#btnMainReviewClassifications');
      await expect(mainReviewBtn).toBeVisible();
      await expect(mainReviewBtn).toHaveText('Review your classifications');

      // Clicking modify locations button opens location modal
      await modifyBtn.click();
      const locModal = page.locator('#locationModal');
      await expect(locModal).toHaveClass(/open/);

      // Close modal
      await page.locator('#locationModal .modal-close').click();
      await expect(locModal).not.toHaveClass(/open/);

      // Clicking main "Review your classifications" button directly opens review modal
      await mainReviewBtn.click();
      const reclassModal = page.locator('#reclassifyModal');
      await expect(reclassModal).toHaveClass(/open/);
      await expect(reclassModal.locator('h2')).toHaveText('Review Your Classifications');
      await page.locator('#reclassifyModal .modal-close').click();
      await expect(reclassModal).not.toHaveClass(/open/);

      // Click location badge to open modal
      await locBadge.click();
      await expect(locModal).toHaveClass(/open/);

      // Input shows current location
      const locInput = page.locator('#locationInput');
      await expect(locInput).toHaveValue('Leh, Ladakh');

      // Update location to "Keoladeo, Bharatpur"
      await locInput.fill('Keoladeo, Bharatpur');

      const cmdBox = page.locator('#locationCommandOutput');
      await expect(cmdBox).toContainText('avicull location --location "Keoladeo, Bharatpur"');

      // Add to batch script
      await page.locator('#btnAddLocationToBatch').click();
      await expect(locModal).not.toHaveClass(/open/);

      // Header batch count increments by 1
      await expect(page.locator('#batchCount')).toHaveText('1');
      await expect(page.locator('#btnDownloadBatch')).toContainText('Download apply_corrections.sh (1)');
    });

    test('supports subdirectory location override scope and populates hierarchical options', async ({ page }) => {
      const locBadge = page.locator('#headerLocationBadge');
      await locBadge.click();
      const locModal = page.locator('#locationModal');
      await expect(locModal).toHaveClass(/open/);

      // Select "Subdirectory / Sub-folder..." scope
      const scopeSelect = page.locator('#locationScopeSelect');
      await scopeSelect.selectOption('directory');

      // Subdirectory selector row becomes visible
      const subdirRow = page.locator('#locationSubdirRow');
      await expect(subdirRow).toBeVisible();

      const subdirSelect = page.locator('#locationSubdirSelect');
      await expect(subdirSelect).toBeVisible();

      // Verify subdirectories from fixture are populated
      const options = await subdirSelect.locator('option').allTextContents();
      expect(options.some(opt => opt.includes('Leh Valley'))).toBe(true);
      expect(options.some(opt => opt.includes('Shey Marshes'))).toBe(true);

      // Choose "Leh Valley/Shey Marshes"
      await subdirSelect.selectOption('Leh Valley/Shey Marshes');

      // Fill in location
      const locInput = page.locator('#locationInput');
      await locInput.fill('Shey, Ladakh');

      // Verify generated command targets the subdirectory with --dir
      const cmdBox = page.locator('#locationCommandOutput');
      await expect(cmdBox).toContainText('avicull location --dir "Leh Valley/Shey Marshes" --location "Shey, Ladakh"');

      // Close modal
      await page.locator('#locationModal .modal-close').click();
      await expect(locModal).not.toHaveClass(/open/);
    });

    test('opens reclassification review diff modal from location dialog and supports Take/Keep toggles', async ({ page }) => {
      const locBadge = page.locator('#headerLocationBadge');
      await locBadge.click();

      const locModal = page.locator('#locationModal');
      await expect(locModal).toHaveClass(/open/);

      // Verify "Review your classifications" button exists and click it
      const reclassifyBtn = page.locator('#btnReclassifyModalTrigger');
      await expect(reclassifyBtn).toBeVisible();
      await expect(reclassifyBtn).toHaveText('Review your classifications');

      // Verify "Reclassify on location change" checkbox is visible and checked by default
      const toggle = page.locator('#reclassifyOnLocationChange');
      await expect(toggle).toBeVisible();
      await expect(toggle).toBeChecked();

      // Enter location and verify command contains --reclassify
      await page.fill('#locationInput', 'Leh, Ladakh');
      await expect(page.locator('#locationCommandOutput')).toContainText('--reclassify');

      // Uncheck toggle -> command omits --reclassify
      await toggle.uncheck();
      await expect(page.locator('#locationCommandOutput')).not.toContainText('--reclassify');

      // Check it back -> command contains --reclassify
      await toggle.check();
      await expect(page.locator('#locationCommandOutput')).toContainText('--reclassify');

      await reclassifyBtn.click();

      // Location modal closes, reclassification review modal opens
      await expect(locModal).not.toHaveClass(/open/);
      const reclassModal = page.locator('#reclassifyModal');
      await expect(reclassModal).toHaveClass(/open/);
      await expect(reclassModal.locator('h2')).toHaveText('Review Your Classifications');

      // Verify diff table rows exist
      const rows = page.locator('#reclassifyDiffTableBody tr');
      await expect(rows).toHaveCount(8);

      // Verify "Take All" and "Keep All" buttons
      const btnTakeAll = page.locator('#btnTakeAll');
      const btnKeepAll = page.locator('#btnKeepAll');
      await expect(btnTakeAll).toBeVisible();
      await expect(btnKeepAll).toBeVisible();

      // Click "Keep All" -> active-keep on all pills
      await btnKeepAll.click();
      await expect(page.locator('.choice-pill.active-keep')).toHaveCount(8);

      // Click "Take All" -> active-take on all pills
      await btnTakeAll.click();
      await expect(page.locator('.choice-pill.active-take')).toHaveCount(8);

      // Add to batch script
      await page.locator('#btnApplyReclassifyBatch').click();
      await expect(reclassModal).not.toHaveClass(/open/);

      // Batch counter increments
      await expect(page.locator('#batchCount')).not.toHaveText('0');
    });
  });
});
