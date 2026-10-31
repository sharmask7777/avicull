import { test, expect } from '@playwright/test';
import path from 'path';

// Validates the live generated HTML catalog from the local mock dataset demo
const DEMO_CATALOG_PATH = `file://${path.resolve('/tmp/avicull_pr23_demo/birds_hierarchy/bird_catalog.html')}`;

test.describe('Avicull Live Mock Dataset Catalog E2E Visual & Interaction Tests', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(DEMO_CATALOG_PATH);
    await page.waitForLoadState('domcontentloaded');
  });

  test('renders all 4 mock dataset photo cards across all trips', async ({ page }) => {
    const cards = page.locator('#galleryGrid .card');
    await expect(cards).toHaveCount(4);

    // Verify stats header
    await expect(page.locator('#statTotal')).toHaveText('4');
    await expect(page.locator('#statConfirmed')).toHaveText('3');
    await expect(page.locator('#statReview')).toHaveText('1');
  });

  test('displays species name, date, and location on distinct lines', async ({ page }) => {
    const cards = page.locator('#galleryGrid .card');

    // Card 1: Rose-ringed Parakeet (K Gudi, 2025-05-18)
    const card1 = cards.nth(0);
    await expect(card1.locator('.card-species-title')).toHaveText('Rose-ringed Parakeet');
    await expect(card1.locator('.card-date')).toHaveText('2025-05-18');
    await expect(card1.locator('.card-location')).toContainText('K Gudi');

    // Card 2: Great Black-backed Gull (New Zealand, 2025-11-24)
    const card2 = cards.nth(1);
    await expect(card2.locator('.card-species-title')).toHaveText('Great Black-backed Gull');
    await expect(card2.locator('.card-date')).toHaveText('2025-11-24');
    await expect(card2.locator('.card-location')).toContainText('New Zealand');

    // Card 4: Needs Review Red-vented Bulbul (Ranthambore, 2025-10-02)
    const card4 = cards.nth(3);
    await expect(card4.locator('.card-species-title')).toHaveText('Red-vented Bulbul');
    await expect(card4.locator('.badge-tier')).toHaveClass(/badge-review/);
    await expect(card4.locator('.card-date')).toHaveText('2025-10-02');
    await expect(card4.locator('.card-location')).toContainText('Ranthambore');
  });

  test('filters cards dynamically by location or species search', async ({ page }) => {
    const searchInput = page.locator('#searchInput');

    // Search by location "Ranthambore"
    await searchInput.fill('Ranthambore');
    let visibleCards = page.locator('#galleryGrid .card');
    await expect(visibleCards).toHaveCount(1);
    await expect(visibleCards.nth(0).locator('.card-species-title')).toHaveText('Red-vented Bulbul');

    // Clear and search by "Parakeet"
    await searchInput.fill('Parakeet');
    visibleCards = page.locator('#galleryGrid .card');
    await expect(visibleCards).toHaveCount(1);
    await expect(visibleCards.nth(0).locator('.card-species-title')).toHaveText('Rose-ringed Parakeet');
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
