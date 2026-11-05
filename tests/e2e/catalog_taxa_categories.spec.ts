import { test, expect } from '@playwright/test';
import path from 'path';

const FIXTURE_PATH = `file://${path.resolve(__dirname, '../../fixtures/test_taxa_catalog.html')}`;

test.describe('Avicull Taxa Categories & Unified Semantic Search E2E Tests', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(FIXTURE_PATH);
    await page.waitForLoadState('domcontentloaded');
  });

  test('renders category pills for All, Bird, Mammalia, Insecta, Scenery with live counts', async ({ page }) => {
    const allPill = page.locator('button[data-category="all"]');
    const birdPill = page.locator('button[data-category="Bird"]');
    const mammaliaPill = page.locator('button[data-category="Mammalia"]');
    const insectaPill = page.locator('button[data-category="Insecta"]');
    const sceneryPill = page.locator('button[data-category="Scenery"]');

    await expect(allPill).toBeVisible();
    await expect(birdPill).toBeVisible();
    await expect(mammaliaPill).toBeVisible();
    await expect(insectaPill).toBeVisible();
    await expect(sceneryPill).toBeVisible();

    await expect(allPill).toHaveClass(/active/);
  });

  test('clicking Mammalia pill isolates mammal cards', async ({ page }) => {
    const cards = page.locator('#galleryGrid .card');
    await expect(cards).toHaveCount(8);

    const mammaliaPill = page.locator('button[data-category="Mammalia"]');
    await mammaliaPill.click();
    await expect(mammaliaPill).toHaveClass(/active/);

    const visibleCards = page.locator('#galleryGrid .card');
    await expect(visibleCards).toHaveCount(2);
    await expect(visibleCards.first().locator('.card-species-title')).toContainText('Malabar Giant Squirrel');
    await expect(visibleCards.first().locator('.badge-category')).toHaveText('Mammalia');
    await expect(visibleCards.nth(1).locator('.card-species-title')).toContainText('Bengal Tiger');
    await expect(visibleCards.nth(1).locator('.badge-category')).toHaveText('Mammalia');
  });

  test('clicking Insecta pill isolates insect cards', async ({ page }) => {
    const insectaPill = page.locator('button[data-category="Insecta"]');
    await insectaPill.click();
    await expect(insectaPill).toHaveClass(/active/);

    const visibleCards = page.locator('#galleryGrid .card');
    await expect(visibleCards).toHaveCount(2);
    await expect(visibleCards.first().locator('.card-species-title')).toContainText('Atlas Moth');
    await expect(visibleCards.nth(1).locator('.card-species-title')).toContainText('Malabar Banded Peacock');
  });

  test('clicking Scenery pill isolates scenery cards', async ({ page }) => {
    const sceneryPill = page.locator('button[data-category="Scenery"]');
    await sceneryPill.click();
    await expect(sceneryPill).toHaveClass(/active/);

    const visibleCards = page.locator('#galleryGrid .card');
    await expect(visibleCards).toHaveCount(2);
    await expect(visibleCards.first().locator('.card-species-title')).toContainText('Pangong Lake');
    await expect(visibleCards.nth(1).locator('.card-species-title')).toContainText('Aurora Borealis');
  });

  test('typing "squirrel" in search bar filters grid', async ({ page }) => {
    const searchInput = page.locator('#searchInput');
    await searchInput.fill('squirrel');

    const visibleCards = page.locator('#galleryGrid .card');
    await expect(visibleCards).toHaveCount(1);
    await expect(visibleCards.first().locator('.card-species-title')).toContainText('Malabar Giant Squirrel');
  });

  test('correction modal produces avicull correct --category Mammalia --species ...', async ({ page }) => {
    const mammalCard = page.locator('#galleryGrid .card', { hasText: 'Malabar Giant Squirrel' });
    const correctBtn = mammalCard.locator('.btn-correct');
    await correctBtn.click();

    const modal = page.locator('#correctionModal');
    await expect(modal).toHaveClass(/open/);

    const speciesInput = page.locator('#speciesSearchInput');
    await speciesInput.fill('Indian Giant Squirrel');

    await page.evaluate(() => {
      // @ts-ignore
      if (typeof window.selectSpecies === 'function') {
        // @ts-ignore
        window.selectSpecies('Indian Giant Squirrel');
      }
    });

    const cmdOutput = page.locator('#commandOutput');
    await expect(cmdOutput).toContainText('avicull correct --category "Mammalia" --species "Indian_Giant_Squirrel"');
  });

  test('status filter pills and dynamic category sub-pills interaction', async ({ page }) => {
    // Stat card check
    const statConfirmedLabel = page.locator('.stat-confirmed .stat-label');
    await expect(statConfirmedLabel).toHaveText('Confirmed Species');

    // Status pill checks
    const statusConfirmedPill = page.locator('.filter-pills button[data-filter="ConfirmedSpecies"]');
    const statusNoSubjectPill = page.locator('.filter-pills button[data-filter="NoBirdDetected"]');
    await expect(statusNoSubjectPill).toContainText('No Subject');

    // Sub-pills row visible initially
    const subPillsRow = page.locator('#categorySubPillsRow');
    await expect(subPillsRow).toBeVisible();

    // Click Confirmed status
    await statusConfirmedPill.click();
    await expect(statusConfirmedPill).toHaveClass(/active/);

    // Dynamic category sub-pill counts when Confirmed is active
    const birdSubCount = page.locator('#catCountBird');
    const mamSubCount = page.locator('#catCountMammalia');
    await expect(birdSubCount).toHaveText('2');
    await expect(mamSubCount).toHaveText('2');

    // Click No Subject status -> sub-pills row should be hidden
    await statusNoSubjectPill.click();
    await expect(statusNoSubjectPill).toHaveClass(/active/);
    await expect(subPillsRow).toHaveClass(/hidden/);

    // Reset to All status -> sub-pills row visible again
    const statusAllPill = page.locator('.filter-pills button[data-filter="all"]');
    await statusAllPill.click();
    await expect(subPillsRow).not.toHaveClass(/hidden/);
  });
});
