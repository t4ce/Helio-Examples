#!/usr/bin/env python3
"""Build Helio's voxel atlases and the complete Brixel showcase.

Every texture-set JSON with readable color, normal, and MER maps is one
64x64 showcase swatch. 128x128 maps are reduced to the showcase resolution;
vertical 64xN animation sheets use their deterministic first frame. Sources
narrower than one tile are reported as excluded in the generated manifest.
"""

from __future__ import annotations

import json
import math
from pathlib import Path

from PIL import Image


ROOT = Path(__file__).resolve().parent
BLOCKS = ROOT / "textures" / "blocks"
OUTPUT = ROOT / "voxel_faces"
NORMAL_OUTPUT = ROOT / "voxel_normals"
MATERIAL_OUTPUT = ROOT / "voxel_materials"
SHOWCASE_OUTPUT = ROOT / "showcase"

TILE_SIZE = 64
SHOWCASE_COLUMNS = 8

LAYOUT = {
    "grass": ("grass_top", "grass_side", "dirt"),
    "dirt": ("dirt", "dirt", "dirt"),
    "stone": ("stone", "stone", "stone"),
    "coal_ore": ("coal_ore", "coal_ore", "coal_ore"),
    "sand": ("sand", "sand", "sand"),
    "stonebrick": ("stonebrick", "stonebrick", "stonebrick"),
    "gravel": ("gravel", "gravel", "gravel"),
    "snow": ("snow", "snow", "snow"),
}


def _resampling_filter():
    # Pillow < 9 exposed these filters directly on Image.
    return getattr(Image, "Resampling", Image).LANCZOS


def _resolve_ref(reference: str, *, color: bool = False) -> Path | None:
    """Resolve a texture-set reference, whose extension is usually omitted."""

    name = Path(reference).name
    candidate = BLOCKS / name
    if candidate.suffix.lower() in {".png", ".tga"}:
        return candidate if candidate.exists() else None
    suffixes = (".tga", ".png") if color else (".png", ".tga")
    for suffix in suffixes:
        candidate = BLOCKS / f"{name}{suffix}"
        if candidate.exists():
            return candidate
    return None


def _normalize_tile(image: Image.Image, path: Path) -> Image.Image:
    """Make one deterministic 64x64 tile from a compatible source image."""

    width, height = image.size
    if width < TILE_SIZE or height < TILE_SIZE:
        raise ValueError(f"{path} is {image.size}; source is narrower than 64x64")
    if width == TILE_SIZE:
        # Bedrock animation sheets are vertical: the first frame is the
        # top-left tile and is stable across runs.
        return image.crop((0, 0, TILE_SIZE, TILE_SIZE))
    if image.size == (TILE_SIZE * 2, TILE_SIZE * 2):
        return image.resize((TILE_SIZE, TILE_SIZE), _resampling_filter())
    raise ValueError(f"{path} is {image.size}; only 64xN or 128x128 is supported")


def _load_image(reference: str, *, color: bool) -> tuple[Image.Image, Path]:
    path = _resolve_ref(reference, color=color)
    if path is None:
        raise FileNotFoundError(f"texture reference {reference!r} is missing")
    try:
        image = Image.open(path).convert("RGBA")
    except Exception as error:
        raise ValueError(f"could not read {path}: {error}") from error
    return _normalize_tile(image, path), path


def load_color(stem: str) -> Image.Image:
    """Load one opaque color map used by the original terrain face atlases."""

    image, _ = _load_image(stem, color=True)
    image.putalpha(255)
    return image


def load_normal(stem: str) -> Image.Image:
    image, _ = _load_image(f"{stem}_normal", color=False)
    return image


def _load_material_reference(reference: str) -> Image.Image:
    """Convert Bedrock MER to Helio's roughness/metalness channel layout.

    Bedrock: R=metalness, G=emissive, B=roughness.
    Helio:   R=unused, G=roughness, B=metalness.
    """

    image, _ = _load_image(reference, color=False)
    source = image.convert("RGB")
    metalness, _emissive, roughness = source.split()
    return Image.merge(
        "RGBA",
        (
            Image.new("L", source.size, 255),
            roughness,
            metalness,
            Image.new("L", source.size, 255),
        ),
    )


def load_helio_material(stem: str) -> Image.Image:
    return _load_material_reference(f"{stem}_mer")


def _texture_set_sources(path: Path) -> tuple[str, str, str]:
    document = json.loads(path.read_text())
    texture_set = document["minecraft:texture_set"]
    return (
        texture_set["color"],
        texture_set["normal"],
        texture_set["metalness_emissive_roughness"],
    )


def _showcase_tiles() -> tuple[list[dict[str, str]], list[dict[str, str]]]:
    """Return ordered complete texture sets and explicit exclusion reasons."""

    included: list[dict[str, str]] = []
    excluded: list[dict[str, str]] = []
    for descriptor in sorted(BLOCKS.glob("*.texture_set.json"), key=lambda item: item.name):
        try:
            color_ref, normal_ref, material_ref = _texture_set_sources(descriptor)
            color, _ = _load_image(color_ref, color=True)
            normal, _ = _load_image(normal_ref, color=False)
            material = _load_material_reference(material_ref)
            if color.size != (TILE_SIZE, TILE_SIZE) or normal.size != (TILE_SIZE, TILE_SIZE) or material.size != (TILE_SIZE, TILE_SIZE):
                raise ValueError("normalization did not produce a 64x64 tile")
            included.append(
                {
                    "name": Path(color_ref).stem,
                    "color": color_ref,
                    "normal": normal_ref,
                    "material": material_ref,
                }
            )
        except (KeyError, json.JSONDecodeError, OSError, ValueError, TypeError) as error:
            excluded.append({"name": descriptor.stem.removesuffix(".texture_set"), "reason": str(error)})
    return included, excluded


def _write_showcase() -> None:
    SHOWCASE_OUTPUT.mkdir(parents=True, exist_ok=True)
    entries, excluded = _showcase_tiles()
    rows = math.ceil(len(entries) / SHOWCASE_COLUMNS)
    width = SHOWCASE_COLUMNS * TILE_SIZE
    height = rows * TILE_SIZE
    showcase_color = Image.new("RGBA", (width, height), (0, 0, 0, 255))
    showcase_normal = Image.new("RGBA", (width, height), (128, 128, 255, 255))
    showcase_material = Image.new("RGBA", (width, height), (255, 255, 0, 255))
    for index, entry in enumerate(entries):
        position = ((index % SHOWCASE_COLUMNS) * TILE_SIZE, (index // SHOWCASE_COLUMNS) * TILE_SIZE)
        color, _ = _load_image(entry["color"], color=True)
        normal, _ = _load_image(entry["normal"], color=False)
        material = _load_material_reference(entry["material"])
        showcase_color.paste(color, position)
        showcase_normal.paste(normal, position)
        showcase_material.paste(material, position)

    showcase_color.save(SHOWCASE_OUTPUT / "color.png", optimize=True)
    showcase_normal.save(SHOWCASE_OUTPUT / "normal.png", optimize=True)
    showcase_material.save(SHOWCASE_OUTPUT / "material.png", optimize=True)
    # Stop when a 64x64 tile reaches one texel; using the atlas's long
    # dimension would incorrectly blend separate showcase rows.
    mip_levels = TILE_SIZE.bit_length()
    manifest = {
        "format": 1,
        "block_count": len(entries),
        "columns": SHOWCASE_COLUMNS,
        "rows": rows,
        "tile_size": TILE_SIZE,
        "width": width,
        "height": height,
        "mip_levels": mip_levels,
        "color": "color.png",
        "normal": "normal.png",
        "material": "material.png",
        "animation_policy": "first 64x64 frame for vertical 64xN sources",
        "ordered_names": [entry["name"] for entry in entries],
        "names": [entry["name"] for entry in entries],
        "excluded": excluded,
    }
    (SHOWCASE_OUTPUT / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    (SHOWCASE_OUTPUT / "count.txt").write_text(f"{len(entries)}\n")
    print(f"showcase: {len(entries)} blocks, {SHOWCASE_COLUMNS}x{rows}, {width}x{height}; excluded {len(excluded)}")


def main() -> None:
    OUTPUT.mkdir(parents=True, exist_ok=True)
    NORMAL_OUTPUT.mkdir(parents=True, exist_ok=True)
    MATERIAL_OUTPUT.mkdir(parents=True, exist_ok=True)
    for name, faces in LAYOUT.items():
        atlas = Image.new("RGBA", (192, 64), (0, 0, 0, 255))
        normal_atlas = Image.new("RGBA", (192, 64), (128, 128, 255, 255))
        material_atlas = Image.new("RGBA", (192, 64), (255, 255, 0, 255))
        for slot, stem in enumerate(faces):
            atlas.paste(load_color(stem), (slot * 64, 0))
            normal_atlas.paste(load_normal(stem), (slot * 64, 0))
            material_atlas.paste(load_helio_material(stem), (slot * 64, 0))
        atlas.save(OUTPUT / f"{name}.png", optimize=True)
        normal_atlas.save(NORMAL_OUTPUT / f"{name}.png", optimize=True)
        material_atlas.save(MATERIAL_OUTPUT / f"{name}.png", optimize=True)
    _write_showcase()


if __name__ == "__main__":
    main()
