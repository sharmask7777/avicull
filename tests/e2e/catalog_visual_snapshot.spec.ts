import { test, expect } from '@playwright/test';
import path from 'path';

// Load the committed reference snapshot
const SNAPSHOT_CATALOG_PATH = `file://${path.resolve(__dirname, '../fixtures/snapshots/bird_catalog_reference.html')}`;

test.describe('Avicull Catalog Visual & Layout Regression Tests', () => {
  test.beforeEach(async ({ page }) => {
    // Navigate to snapshot catalog
    await page.goto(SNAPSHOT_CATALOG_PATH);
    await page.waitForLoadState('domcontentloaded');
  });

  test('matches visual snapshot for main catalog header and controls', async ({ page }) => {
    // Assert visual snapshot of the header region (title, stats, location, buttons)
    const header = page.locator('header');
    await expect(header).toBeVisible();
    await expect(header).toHaveScreenshot('catalog-header-layout.png');
  });

  test('matches visual snapshot for bird cards grid', async ({ page }) => {
    // Assert visual snapshot of gallery grid
    const grid = page.locator('#galleryGrid');
    await expect(grid).toBeVisible();
    await expect(grid).toHaveScreenshot('catalog-gallery-grid-layout.png');
  });

  test('matches visual snapshot for review classifications modal', async ({ page }) => {
    // Open classification review modal
    const reviewBtn = page.locator('#btnMainReviewClassifications');
    await reviewBtn.click();

    const modalBox = page.locator('#reclassifyModal .modal-box');
    await expect(modalBox).toBeVisible();
    await expect(modalBox).toHaveScreenshot('catalog-review-modal-layout.png');
  });

  test('matches visual snapshot for location modal', async ({ page }) => {
    // Open location modal
    const modifyBtn = page.locator('#btnModifyLocations');
    await modifyBtn.click();

    const modalBox = page.locator('#locationModal .modal-box');
    await expect(modalBox).toBeVisible();
    await expect(modalBox).toHaveScreenshot('catalog-location-modal-layout.png');
  });
});
