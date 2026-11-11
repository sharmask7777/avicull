import { test, expect } from '@playwright/test';
import * as path from 'path';

const CATALOG_PATH = `file://${path.resolve(__dirname, '../../target/mock_run_output/bird_catalog.html')}`;

test.describe('Multi-Category Override Flow', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(CATALOG_PATH);
    await page.waitForLoadState('domcontentloaded');
  });

  test('multi-category correction modal and autocomplete', async ({ page }) => {
    // Open correction modal for first photo card
    const correctBtn = page.locator('.btn-correct').first();
    await correctBtn.click();

    const modal = page.locator('#correctionModal');
    await expect(modal).toBeVisible();

    // Verify modal title is updated
    await expect(modal.locator('.modal-header h2')).toHaveText('Correct Classification');

    // Verify category pills are rendered
    const mammaliaPill = modal.locator('.cat-pill', { hasText: 'Mammalia' });
    await expect(mammaliaPill).toBeVisible();

    const sceneryPill = modal.locator('.cat-pill', { hasText: 'Scenery' });
    await expect(sceneryPill).toBeVisible();

    // 1. Click Mammalia pill and test autocomplete
    await mammaliaPill.click();
    await expect(mammaliaPill).toHaveClass(/active/);

    const searchInput = modal.locator('#speciesSearchInput');
    await searchInput.fill('Snow');

    const sugItem = modal.locator('.suggestion-item', { hasText: 'Snow Leopard' });
    await expect(sugItem).toBeVisible();
    await sugItem.click();

    // Verify generated command in command box
    const commandBox = modal.locator('#commandOutput');
    await expect(commandBox).toContainText('--category "Mammalia"');
    await expect(commandBox).toContainText('--species "Snow_Leopard"');

    // 2. Click Scenery pill with empty species (top-level genre)
    await sceneryPill.click();
    await expect(sceneryPill).toHaveClass(/active/);
    await searchInput.fill('');

    await expect(commandBox).toContainText('--category "Scenery"');
    await expect(commandBox).not.toContainText('--species');

    // 3. Click No Subject pill
    const noSubjectPill = modal.locator('.cat-pill', { hasText: 'No Subject' });
    await expect(noSubjectPill).toBeVisible();
    await noSubjectPill.click();
    await expect(noSubjectPill).toHaveClass(/active/);
    await expect(searchInput).toBeDisabled();
    await expect(commandBox).toContainText('_No_Subject_Detected');
  });
});
