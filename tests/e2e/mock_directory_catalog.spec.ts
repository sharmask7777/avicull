import { test, expect } from '@playwright/test';
import fs from 'fs';
import path from 'path';

// Validates the live generated HTML catalog from the local mock dataset demo (in-place or hierarchy)
// Falls back to target/mock_run_output or committed reference snapshot when a live run demo directory is not active in /tmp
const INPLACE_PATH = path.resolve('/tmp/avicull_pr23_demo/bird_catalog.html');
const HIERARCHY_PATH = path.resolve('/tmp/avicull_pr23_demo/birds_hierarchy/bird_catalog.html');
const TARGET_MOCK_PATH = path.resolve(__dirname, '../../target/mock_run_output/bird_catalog.html');
const SNAPSHOT_PATH = path.resolve(__dirname, '../fixtures/snapshots/bird_catalog_reference.snap');

const RESOLVED_PATH = fs.existsSync(TARGET_MOCK_PATH)
  ? TARGET_MOCK_PATH
  : fs.existsSync(INPLACE_PATH)
  ? INPLACE_PATH
  : fs.existsSync(HIERARCHY_PATH)
  ? HIERARCHY_PATH
  : SNAPSHOT_PATH;

const DEMO_CATALOG_PATH = `file://${RESOLVED_PATH}`;

test.describe('Mock Directory Structure Catalog Visual & Interaction Tests', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(DEMO_CATALOG_PATH);
    await page.waitForLoadState('domcontentloaded');
  });

  test('renders all 7 mock dataset photo cards across all trips and categories', async ({ page }) => {
    const cards = page.locator('#galleryGrid .card');
    await expect(cards).toHaveCount(7);

    // Verify stats header
    await expect(page.locator('#statTotal')).toHaveText('7');
    await expect(page.locator('#statConfirmed')).toHaveText('7');
    await expect(page.locator('#statReview')).toHaveText('0');
  });

  test('displays species name, date, and location on distinct lines', async ({ page }) => {
    const cards = page.locator('#galleryGrid .card');

    // Card 1: Rose-ringed Parakeet (K Gudi)
    const card1 = cards.nth(0);
    await expect(card1.locator('.card-species-title')).toContainText('Rose-ringed Parakeet');
    await expect(card1.locator('.card-date')).not.toBeEmpty();
    await expect(card1.locator('.card-location')).toContainText('K Gudi');

    // Card 2: Kelp Gull (New Zealand regional taxon)
    const card2 = cards.nth(1);
    await expect(card2.locator('.card-species-title')).toContainText('Kelp Gull');
    await expect(card2.locator('.card-date')).not.toBeEmpty();
    await expect(card2.locator('.card-location')).toContainText('New Zealand');

    // Mammalia card: Malabar Giant Squirrel
    const squirrelCard = page.locator('#galleryGrid .card').filter({ hasText: 'Malabar Giant Squirrel' });
    await expect(squirrelCard).toHaveCount(1);
    await expect(squirrelCard.locator('.badge-category')).toHaveText('Mammalia');

    // Insecta card: Peacock Butterfly
    const insectCard = page.locator('#galleryGrid .card').filter({ hasText: 'Peacock Butterfly' });
    await expect(insectCard).toHaveCount(1);
    await expect(insectCard.locator('.badge-category')).toHaveText('Insecta');

    // Scenery card: Mountain Landscape
    const sceneryCard = page.locator('#galleryGrid .card').filter({ hasText: 'Mountain Landscape' });
    await expect(sceneryCard).toHaveCount(1);
    await expect(sceneryCard.locator('.badge-category')).toHaveText('Scenery');
  });

  test('filters cards dynamically by location or species search', async ({ page }) => {
    const searchInput = page.locator('#searchInput');

    // Search by location "Ranthambore"
    await searchInput.fill('Ranthambore');
    let visibleCards = page.locator('#galleryGrid .card');
    await expect(visibleCards).toHaveCount(1);
    await expect(visibleCards.nth(0).locator('.card-species-title')).toContainText('Red-vented Bulbul');

    // Clear and search by "Parakeet"
    await searchInput.fill('Parakeet');
    visibleCards = page.locator('#galleryGrid .card');
    await expect(visibleCards).toHaveCount(1);
    await expect(visibleCards.nth(0).locator('.card-species-title')).toContainText('Rose-ringed Parakeet');
  });

  test('opens bulk location modal with hierarchical subdirectories and generates commands', async ({ page }) => {
    const locationBtn = page.locator('#headerLocationBadge');
    await locationBtn.click();

    const locModal = page.locator('#locationModal');
    await expect(locModal).toHaveClass(/open/);

    // Switch to Directory Overrides tab
    const bulkTab = page.locator('#tabBulk');
    await bulkTab.click();

    // Verify initial bulk row is created
    const rows = page.locator('#bulkLocationRows .bulk-loc-row');
    await expect(rows).toHaveCount(1);

    // Select subdirectory from options (first non-empty option)
    const select0 = rows.nth(0).locator('select');
    await select0.selectOption({ index: 1 });

    // Fill in location override for first row
    const input0 = rows.nth(0).locator('input[type="text"]');
    await input0.fill('11.9960, 77.1350');

    // Verify generated command block updates with avicull location command
    const commandText = page.locator('#bulkLocationCommandOutput');
    await expect(commandText).toContainText('avicull location');
    await expect(commandText).toContainText('11.9960, 77.1350');
  });
});
