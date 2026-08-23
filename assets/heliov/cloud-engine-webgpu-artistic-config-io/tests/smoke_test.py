#!/usr/bin/env python3
"""Optional browser smoke test for Cloud Engine.

Example on a Linux CI host with no display:
  xvfb-run -a python3 tests/smoke_test.py --headed
"""

from __future__ import annotations

import argparse
import asyncio
import json
import tempfile
from pathlib import Path

from playwright.async_api import ConsoleMessage, Error, Page, async_playwright


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--url", default="http://127.0.0.1:8080/")
    parser.add_argument("--chromium", default="/usr/bin/chromium")
    parser.add_argument("--headed", action="store_true", help="Use a visible browser; useful for software WebGPU under Xvfb")
    parser.add_argument("--screenshot", default="artifacts/smoke.png")
    return parser.parse_args()


async def wait_for_engine(page: Page) -> None:
    await page.wait_for_function(
        """() => {
          const value = document.querySelector('#gpuStatus')?.textContent || '';
          return value.includes('ready') || value.includes('READY') || value.includes('Unavailable') || value.includes('UNAVAILABLE');
        }""",
        timeout=30_000,
    )
    status = (await page.locator("#gpuStatus").inner_text()).strip().upper()
    if "READY" not in status:
        message = await page.locator("#unsupportedMessage").inner_text()
        raise RuntimeError(f"WebGPU did not initialize: {status}: {message}")


async def exercise_controls(page: Page) -> None:
    await page.select_option("#patternSelect", "4")
    await page.locator("#cloudAmount").fill("0.43")
    await page.locator("#windDirection").fill("-34")
    await page.locator("#tearing").fill("0.52")
    await page.locator("#sunColor").fill("#ffd7ad")
    await page.locator("#qualitySelect").select_option("eco")

    await page.locator("#modeDraw").click()
    await page.locator("#pauseButton").click()
    await page.locator("#brushDepth").fill("0.58")

    canvas_box = await page.locator("#cloudCanvas").bounding_box()
    if not canvas_box:
        raise RuntimeError("Canvas has no layout box")

    start_x = canvas_box["x"] + canvas_box["width"] * 0.66
    start_y = canvas_box["y"] + canvas_box["height"] * 0.57
    await page.mouse.move(start_x, start_y)
    await page.mouse.down()
    for offset in range(0, 151, 30):
        await page.mouse.move(start_x + offset, start_y - offset * 0.18, steps=3)
        await page.wait_for_timeout(40)
    await page.mouse.up()

    # Brush depth and erase paths.
    await page.mouse.wheel(0, -120)
    await page.keyboard.down("Shift")
    await page.mouse.move(start_x + 75, start_y + 30)
    await page.mouse.down()
    await page.mouse.move(start_x + 125, start_y + 10, steps=5)
    await page.mouse.up()
    await page.keyboard.up("Shift")

    await page.locator("#pauseButton").click()
    await page.wait_for_timeout(180)

    await page.locator("#collapsePanel").click()
    await page.wait_for_timeout(150)
    assert await page.locator("#controlPanel").evaluate("element => element.classList.contains('is-collapsed')")
    await page.keyboard.press("h")
    await page.wait_for_timeout(150)
    assert not await page.locator("#controlPanel").evaluate("element => element.classList.contains('is-collapsed')")

    await page.locator("#modeAuto").click()
    await page.locator("#pauseButton").click()
    await page.wait_for_timeout(180)


async def exercise_configuration_io(page: Page) -> None:
    await page.select_option("#styleSelect", "artistic")
    await page.select_option("#patternSelect", "6")
    await page.locator("#cloudAmount").fill("0.37")
    await page.locator("#artCloudColor").fill("#4fae93")
    await page.locator("#artOutline").fill("0.71")
    await page.locator("#cameraFov").fill("63")
    await page.locator("#sunAzimuth").fill("-71")
    await page.select_option("#qualitySelect", "still")

    async with page.expect_download() as download_info:
        await page.locator("#exportConfigButton").click()
    download = await download_info.value
    downloaded_path = await download.path()
    if downloaded_path is None:
        raise RuntimeError("Configuration export did not produce a readable download")

    exported = json.loads(Path(downloaded_path).read_text(encoding="utf-8"))
    assert exported["format"] == "cloud-engine-config"
    assert exported["version"] == 1
    assert exported["engineVersion"] == "2.1-config-io"
    configuration = exported["configuration"]
    assert configuration["formation"]["pattern"] == 6
    assert abs(configuration["formation"]["amount"] - 0.37) < 1e-6
    assert configuration["look"]["renderStyle"] == "artistic"
    assert configuration["look"]["artCloudColor"] == "#4fae93"
    assert configuration["camera"]["cameraFov"] == 63
    assert configuration["sunAndRender"]["quality"] == "still"

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

    with tempfile.TemporaryDirectory() as temporary_directory:
        config_path = Path(temporary_directory) / "roundtrip.cloud.json"
        config_path.write_text(json.dumps(exported, indent=2), encoding="utf-8")
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
    console_errors: list[str] = []
    page_errors: list[str] = []

    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(
            executable_path=args.chromium,
            headless=not args.headed,
            args=[
                "--no-sandbox",
                "--disable-dev-shm-usage",
                "--disable-gpu-sandbox",
                "--enable-unsafe-webgpu",
                "--use-webgpu-adapter=swiftshader",
                "--enable-dawn-features=allow_unsafe_apis",
                "--enable-webgpu-developer-features",
                "--use-gpu-in-tests",
                "--enable-gpu-rasterization",
            ],
        )
        page = await browser.new_page(viewport={"width": 800, "height": 450})

        def on_console(message: ConsoleMessage) -> None:
            if message.type == "error":
                console_errors.append(message.text)

        page.on("console", on_console)
        page.on("pageerror", lambda error: page_errors.append(str(error)))

        try:
            response = await page.goto(args.url, wait_until="networkidle", timeout=30_000)
            if response is None or not response.ok:
                raise RuntimeError(f"Navigation failed: {response.status if response else 'no response'}")
            print("WebGPU initialized", flush=True)
            await wait_for_engine(page)
            await page.wait_for_timeout(700)
            print("Exercising controls", flush=True)
            await exercise_controls(page)
            print("Testing JSON configuration round-trip", flush=True)
            await exercise_configuration_io(page)

            screenshot = Path(args.screenshot)
            screenshot.parent.mkdir(parents=True, exist_ok=True)
            await page.screenshot(path=str(screenshot))

            if page_errors:
                raise RuntimeError("Page errors:\n" + "\n".join(page_errors))
            if console_errors:
                raise RuntimeError("Console errors:\n" + "\n".join(console_errors))

            print(f"PASS: WebGPU ready, controls exercised, screenshot: {screenshot}")
        finally:
            await browser.close()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except (Error, RuntimeError, AssertionError) as error:
        raise SystemExit(f"FAIL: {error}") from error
