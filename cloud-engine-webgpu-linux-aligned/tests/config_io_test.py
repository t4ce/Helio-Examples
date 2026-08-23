#!/usr/bin/env python3
"""Exercise Cloud Engine JSON export/import without localhost or WebGPU."""

from __future__ import annotations

import argparse
import asyncio
import json
import tempfile
from pathlib import Path

from playwright.async_api import Error, Page, async_playwright


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--chromium", default="/usr/bin/chromium")
    return parser.parse_args()


def build_inline_page(root: Path) -> str:
    html = (root / "index.html").read_text(encoding="utf-8")
    javascript = (root / "app.js").read_text(encoding="utf-8")
    html = html.replace('<link rel="stylesheet" href="styles.css" />', "")
    html = html.replace('<link rel="icon" href="favicon.svg" type="image/svg+xml" />', "")
    marker = '<script src="app.js" defer></script>'
    if marker not in html:
        raise RuntimeError("Could not locate app.js script tag")
    return html.replace(marker, f"<script>{javascript}</script>")


async def set_initial_values(page: Page) -> None:
    await page.select_option("#styleSelect", "artistic")
    await page.select_option("#patternSelect", "6")
    await page.locator("#cloudAmount").fill("0.37")
    await page.locator("#artCloudColor").fill("#4fae93")
    await page.locator("#artOutline").fill("0.71")
    await page.locator("#cameraFov").fill("63")
    await page.locator("#sunAzimuth").fill("-71")
    await page.select_option("#qualitySelect", "still")


async def export_document(page: Page) -> dict:
    async with page.expect_download() as download_info:
        await page.locator("#exportConfigButton").click()
    download = await download_info.value
    path = await download.path()
    if path is None:
        raise RuntimeError("Export JSON did not create a readable download")
    document = json.loads(Path(path).read_text(encoding="utf-8"))
    assert document["format"] == "cloud-engine-config"
    assert document["version"] == 1
    assert document["engineVersion"] == "2.1-config-io"
    configuration = document["configuration"]
    assert configuration["formation"]["pattern"] == 6
    assert abs(configuration["formation"]["amount"] - 0.37) < 1e-6
    assert configuration["look"]["renderStyle"] == "artistic"
    assert configuration["look"]["artCloudColor"] == "#4fae93"
    assert configuration["camera"]["cameraFov"] == 63
    assert configuration["sunAndRender"]["quality"] == "still"
    return document


def modify_every_group(document: dict) -> None:
    configuration = document["configuration"]
    configuration["mode"] = "draw"
    configuration["formation"].update({"pattern": 5, "amount": 0.31, "seed": 42.25})
    configuration["look"].update({
        "renderStyle": "artistic",
        "artPreset": "custom",
        "artCloudColor": "#6bb29c",
        "artShadowColor": "#123f49",
        "artSkyColor": "#102c3a",
        "artMoonColor": "#f6dd82",
        "artCurl": 1.36,
        "artRibbon": 0.91,
        "artSculpt": 0.86,
        "artBands": 6,
        "artOutline": 0.83,
        "artMoonSize": 0.205,
        "artMoonGlow": 1.44,
        "artGrain": 0.27,
    })
    configuration["air"].update({
        "windSpeed": 1.24,
        "windDirection": -48,
        "turbulence": 0.93,
        "tearing": 0.66,
        "rotation": -0.21,
        "dissipation": 0.17,
    })
    configuration["brush"].update({
        "brushSize": 0.135,
        "brushStrength": 0.81,
        "brushDepth": 0.63,
    })
    configuration["camera"].update({
        "cameraPosition": [1.25, 2.5, -3.75],
        "cameraYaw": 0.45,
        "cameraPitch": -0.2,
        "cameraFov": 67,
        "cameraSpeed": 6.4,
        "cameraSensitivity": 0.19,
    })
    configuration["sunAndRender"].update({
        "sunColor": "#ffd19a",
        "sunElevation": 14,
        "sunAzimuth": 96,
        "sunIntensity": 1.72,
        "exposure": 1.18,
        "quality": "eco",
    })
    configuration["playback"]["paused"] = True


async def load_document(page: Page, document: dict) -> None:
    with tempfile.TemporaryDirectory() as temporary_directory:
        config_path = Path(temporary_directory) / "roundtrip.cloud.json"
        config_path.write_text(json.dumps(document, indent=2), encoding="utf-8")
        await page.locator("#configFileInput").set_input_files(str(config_path))
        await page.wait_for_function(
            "() => document.querySelector('#toast')?.textContent.includes('Loaded roundtrip.cloud.json')",
            timeout=10_000,
        )

    loaded = await page.evaluate("window.__cloudEngineDebug.getConfiguration().configuration")
    assert loaded["mode"] == "draw"
    assert loaded["formation"]["pattern"] == 5
    assert abs(loaded["formation"]["amount"] - 0.31) < 1e-6
    assert abs(loaded["formation"]["seed"] - 42.25) < 1e-6
    assert loaded["look"]["artPreset"] == "custom"
    assert loaded["look"]["artMoonColor"] == "#f6dd82"
    assert abs(loaded["air"]["windSpeed"] - 1.24) < 1e-6
    assert abs(loaded["brush"]["brushDepth"] - 0.63) < 1e-6
    assert loaded["camera"]["cameraPosition"] == [1.25, 2.5, -3.75]
    assert abs(loaded["camera"]["cameraYaw"] - 0.45) < 1e-6
    assert loaded["camera"]["cameraFov"] == 67
    assert loaded["sunAndRender"]["sunColor"] == "#ffd19a"
    assert loaded["sunAndRender"]["quality"] == "eco"
    assert loaded["playback"]["paused"] is True


async def main() -> None:
    args = parse_args()
    root = Path(__file__).resolve().parents[1]
    html = build_inline_page(root)

    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(
            executable_path=args.chromium,
            headless=True,
            args=["--no-sandbox", "--disable-dev-shm-usage"],
        )
        page = await browser.new_page(accept_downloads=True, viewport={"width": 900, "height": 700})
        try:
            await page.set_content(html, wait_until="load")
            await page.wait_for_function("() => window.__cloudEngineDebug?.version === '2.1-config-io'")
            await set_initial_values(page)
            document = await export_document(page)
            modify_every_group(document)
            await load_document(page, document)
            print("PASS: JSON configuration export/import round-trip")
        finally:
            await browser.close()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except (Error, RuntimeError, AssertionError) as error:
        raise SystemExit(f"FAIL: {error}") from error
