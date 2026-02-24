#!/usr/bin/env node
/**
 * CLI Test Runner for SubsetJuliaVM Web
 *
 * Usage:
 *   node test-runner.js [--server-url=http://localhost:8080]
 *
 * Requirements:
 *   npm install playwright
 *
 * Or run with npx:
 *   npx playwright test-runner.js
 */

const { chromium } = require('playwright');

const SERVER_URL = process.argv.find(a => a.startsWith('--server-url='))?.split('=')[1]
    || process.env.SERVER_URL
    || 'http://localhost:8080';

async function runTests() {
    console.log('SubsetJuliaVM Web Test Runner');
    console.log('========================\n');
    console.log(`Server URL: ${SERVER_URL}\n`);

    const browser = await chromium.launch({ headless: true });
    const page = await browser.newPage();

    // Collect console logs
    page.on('console', msg => {
        if (msg.type() === 'error') {
            console.error('  [Browser Error]', msg.text());
        }
    });

    try {
        // Navigate to test page
        console.log('Loading test page...');
        await page.goto(`${SERVER_URL}/test.html`, { waitUntil: 'networkidle' });

        // Wait for initialization
        await page.waitForFunction(() => {
            const status = document.getElementById('status');
            return status && status.textContent === 'Ready';
        }, { timeout: 30000 });

        console.log('Running tests...\n');

        // Click run all button
        await page.click('#run-all');

        // Wait for tests to complete
        await page.waitForFunction(() => {
            const status = document.getElementById('status');
            return status && status.textContent === 'Done';
        }, { timeout: 120000 });

        // Get results
        const results = await page.evaluate(() => {
            const items = document.querySelectorAll('.test-item');
            const results = [];

            items.forEach((item, idx) => {
                const name = item.querySelector('.name').textContent;
                const status = item.classList.contains('pass') ? 'pass' :
                              item.classList.contains('fail') ? 'fail' : 'unknown';
                const error = item.querySelector('.error')?.textContent || null;

                results.push({ idx, name, status, error });
            });

            return results;
        });

        // Print results
        let passed = 0;
        let failed = 0;

        for (const result of results) {
            const icon = result.status === 'pass' ? '✓' : '✗';
            const color = result.status === 'pass' ? '\x1b[32m' : '\x1b[31m';
            const reset = '\x1b[0m';

            console.log(`${color}${icon}${reset} ${result.name}`);

            if (result.error) {
                console.log(`  ${'\x1b[31m'}Error: ${result.error}${'\x1b[0m'}\n`);
            }

            if (result.status === 'pass') passed++;
            else failed++;
        }

        // Summary
        console.log('\n========================');
        console.log(`Results: ${passed} passed, ${failed} failed, ${results.length} total`);

        await browser.close();

        // Exit with error code if any tests failed
        process.exit(failed > 0 ? 1 : 0);

    } catch (error) {
        console.error('Test runner error:', error.message);
        await browser.close();
        process.exit(1);
    }
}

runTests();
