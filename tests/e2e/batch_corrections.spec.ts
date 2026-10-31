import { test, expect } from '@playwright/test';
import path from 'path';

const FIXTURE_PATH = `file://${path.resolve(__dirname, '../../fixtures/test_bird_catalog.html')}`;

test.describe('Batch Corrections Queue & Sequential Updates (Issue #20 & #25)', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(FIXTURE_PATH);
    await page.waitForLoadState('domcontentloaded');
  });

  test('consecutive corrections update batchCount, copy button, and batch script correctly', async ({ page }) => {
    // 1. Initially batch count is 0
    await expect(page.locator('#batchCount')).toHaveText('0');
    await expect(page.locator('#batchCountCopy')).toHaveText('0');

    // 2. Open first card (bundle-1)
    await page.locator('#card-bundle-1 .btn-correct').click();
    await expect(page.locator('#correctionModal')).toHaveClass(/open/);

    // Select "Painted Stork"
    await page.locator('#speciesSearchInput').fill('Painted Stork');
    await page.locator('#speciesSuggestions .suggestion-item:has-text("Painted Stork")').click();
    await page.locator('#btnAddToBatch').click();
    await expect(page.locator('#correctionModal')).not.toHaveClass(/open/);

    // Batch count should now be 1
    await expect(page.locator('#batchCount')).toHaveText('1');
    await expect(page.locator('#batchCountCopy')).toHaveText('1');

    // 3. Open second card (bundle-2)
    await page.locator('#card-bundle-2 .btn-correct').click();
    await expect(page.locator('#correctionModal')).toHaveClass(/open/);

    // Select "House Sparrow"
    await page.locator('#speciesSearchInput').fill('Sparrow');
    await page.locator('#speciesSuggestions .suggestion-item:has-text("House Sparrow")').click();
    await page.locator('#btnAddToBatch').click();
    await expect(page.locator('#correctionModal')).not.toHaveClass(/open/);

    // Batch count should now be 2
    await expect(page.locator('#batchCount')).toHaveText('2');
    await expect(page.locator('#batchCountCopy')).toHaveText('2');
    await expect(page.locator('#btnDownloadBatch')).toContainText('Download apply_corrections.sh (2)');
    await expect(page.locator('#btnCopyBatchScript')).toContainText('Copy Batch Script (2)');
  });

  test('single photo command copy does not contain raw comment lines starting with #', async ({ page }) => {
    await page.locator('#card-bundle-1 .btn-correct').click();
    await expect(page.locator('#correctionModal')).toHaveClass(/open/);

    await page.locator('#speciesSearchInput').fill('Painted Stork');
    await page.locator('#speciesSuggestions .suggestion-item:has-text("Painted Stork")').click();

    // Verify commandOutput has comments
    const cmdText = await page.locator('#commandOutput').textContent();
    expect(cmdText).toContain('# Locate avicull binary');

    // Call copyCurrentCommand and intercept clipboard
    const copiedText = await page.evaluate(async () => {
      let clipboard = '';
      const origWrite = navigator.clipboard.writeText;
      navigator.clipboard.writeText = async (text) => {
        clipboard = text;
        return Promise.resolve();
      };
      // @ts-ignore
      copyCurrentCommand();
      navigator.clipboard.writeText = origWrite;
      return clipboard;
    });

    // The copied command MUST NOT have lines starting with '#'
    const lines = copiedText.split('\n').map(l => l.trim());
    const commentLines = lines.filter(l => l.startsWith('#'));
    expect(commentLines.length).toBe(0);
    expect(copiedText).toContain('avicull correct --file');
  });
});
