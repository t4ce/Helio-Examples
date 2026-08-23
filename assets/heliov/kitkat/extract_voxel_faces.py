"""Bake the fixed HelioV Block Bits face set.

Run from the repository root:

    blender --background --factory-startup --python \
      crates/examples/assets/heliov/kitkat/extract_voxel_faces.py

Every output is a 192x64 RGBA image with exactly this layout:

    [ top 64x64 | side 64x64 | bottom 64x64 ]

The images are design-time assets. Helio never runs Blender or reads the source
GLTF files at runtime.
"""

from pathlib import Path
import math
from collections import deque

import bpy
from mathutils import Vector


ROOT = Path(__file__).resolve().parent
SOURCE = ROOT / "Assets" / "gltf"
OUTPUT = ROOT / "voxel_faces"
TILE = 64

# This is the complete, ordered HelioV starter palette. Do not discover files
# from the directory: changing this list is an intentional asset decision.
BLOCKS = (
    ("grass", "dirt_with_grass.gltf"),
    ("dirt", "dirt.gltf"),
    ("stone", "stone.gltf"),
    ("gold_ore", "stone_with_gold.gltf"),
    ("sand", "sand_A.gltf"),
    ("bricks", "bricks_A.gltf"),
    ("gravel", "gravel.gltf"),
    ("snow", "snow.gltf"),
)


def clear_scene():
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    for datablocks in (bpy.data.meshes, bpy.data.materials, bpy.data.images):
        for datablock in list(datablocks):
            if datablock.users == 0:
                datablocks.remove(datablock)


def world_bounds(objects):
    corners = [obj.matrix_world @ Vector(corner) for obj in objects for corner in obj.bound_box]
    low = Vector((min(p.x for p in corners), min(p.y for p in corners), min(p.z for p in corners)))
    high = Vector((max(p.x for p in corners), max(p.y for p in corners), max(p.z for p in corners)))
    return low, high


def configure_scene():
    scene = bpy.context.scene
    scene.render.engine = "BLENDER_EEVEE"
    scene.render.resolution_x = TILE
    scene.render.resolution_y = TILE
    scene.render.resolution_percentage = 100
    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_mode = "RGBA"
    scene.render.film_transparent = True
    scene.view_settings.look = "AgX - Medium High Contrast"

    world = bpy.data.worlds.new("HelioV neutral world")
    world.use_nodes = True
    background = world.node_tree.nodes["Background"]
    background.inputs["Color"].default_value = (0.8, 0.8, 0.8, 1.0)
    background.inputs["Strength"].default_value = 0.8
    scene.world = world

    light_data = bpy.data.lights.new("HelioV key", type="AREA")
    light_data.energy = 700.0
    light_data.shape = "DISK"
    light_data.size = 5.0
    light = bpy.data.objects.new("HelioV key", light_data)
    light.location = (4.0, -5.0, 7.0)
    scene.collection.objects.link(light)

    camera_data = bpy.data.cameras.new("HelioV face camera")
    camera_data.type = "ORTHO"
    camera = bpy.data.objects.new("HelioV face camera", camera_data)
    scene.collection.objects.link(camera)
    scene.camera = camera
    return scene, camera


def point_camera(camera, center, direction, up):
    camera.location = center + direction * 10.0
    camera.rotation_euler = (center - camera.location).to_track_quat("-Z", up).to_euler()


def make_materials_unlit(meshes):
    """Keep KayKit color/UV detail but remove lighting baked into face PNGs."""
    for material in {slot.material for obj in meshes for slot in obj.material_slots if slot.material}:
        material.use_nodes = True
        nodes = material.node_tree.nodes
        links = material.node_tree.links
        image_node = next((node for node in nodes if node.type == "TEX_IMAGE"), None)
        if image_node is None:
            continue
        output = next((node for node in nodes if node.type == "OUTPUT_MATERIAL"), None)
        emission = nodes.new("ShaderNodeEmission")
        emission.inputs["Strength"].default_value = 1.0
        links.new(image_node.outputs["Color"], emission.inputs["Color"])
        links.new(emission.outputs["Emission"], output.inputs["Surface"])


def render_tile(scene, camera, center, extent, direction, up, path):
    point_camera(camera, center, Vector(direction), up)
    camera.data.ortho_scale = extent * 1.08
    scene.render.filepath = str(path)
    bpy.ops.render.render(write_still=True)


def join_tiles(paths, output):
    tiles = [bpy.data.images.load(str(path), check_existing=False) for path in paths]
    atlas = bpy.data.images.new(output.stem, width=TILE * 3, height=TILE, alpha=True)
    pixels = [0.0] * (TILE * TILE * 3 * 4)
    for tile_index, tile in enumerate(tiles):
        source = make_opaque(list(tile.pixels))
        for y in range(TILE):
            # Existing cube UVs intentionally put v=0 at the bottom of every
            # vertical face. Store only the side region bottom-up so its visual
            # top (grass/snow caps) lands at the cube top without touching the
            # renderer or expanding the atlas to four directional sides.
            source_y = TILE - 1 - y if tile_index == 1 else y
            src = source_y * TILE * 4
            dst = (y * TILE * 3 + tile_index * TILE) * 4
            pixels[dst : dst + TILE * 4] = source[src : src + TILE * 4]
    atlas.pixels = pixels
    atlas.filepath_raw = str(output)
    atlas.file_format = "PNG"
    atlas.save()
    for tile in tiles:
        bpy.data.images.remove(tile)


def make_opaque(source):
    """Extend the rendered face to every texel; voxel faces stay solid squares."""
    queue = deque()
    seen = [False] * (TILE * TILE)
    for index in range(TILE * TILE):
        if source[index * 4 + 3] > 0.5:
            queue.append(index)
            seen[index] = True
    while queue:
        index = queue.popleft()
        x, y = index % TILE, index // TILE
        for nx, ny in ((x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)):
            if not (0 <= nx < TILE and 0 <= ny < TILE):
                continue
            neighbor = ny * TILE + nx
            if seen[neighbor]:
                continue
            source[neighbor * 4 : neighbor * 4 + 3] = source[index * 4 : index * 4 + 3]
            seen[neighbor] = True
            queue.append(neighbor)
    for index in range(TILE * TILE):
        source[index * 4 + 3] = 1.0
    return source


def bake_block(name, source_name, scene, camera):
    clear_scene()
    # clear_scene also removes the camera/light, so recreate render state for
    # every asset. This prevents imported scenes from leaking state.
    scene, camera = configure_scene()
    bpy.ops.import_scene.gltf(filepath=str(SOURCE / source_name))
    meshes = [obj for obj in scene.objects if obj.type == "MESH"]
    make_materials_unlit(meshes)
    low, high = world_bounds(meshes)
    center = (low + high) * 0.5
    extent = max(high.x - low.x, high.y - low.y, high.z - low.z)

    temporary = [OUTPUT / f".{name}-{face}.png" for face in ("top", "side", "bottom")]
    render_tile(scene, camera, center, extent, (0, 0, 1), "Y", temporary[0])
    render_tile(scene, camera, center, extent, (0, -1, 0), "Z", temporary[1])
    render_tile(scene, camera, center, extent, (0, 0, -1), "Y", temporary[2])
    join_tiles(temporary, OUTPUT / f"{name}.png")
    for path in temporary:
        path.unlink()


OUTPUT.mkdir(parents=True, exist_ok=True)
scene = camera = None
for block_name, gltf_name in BLOCKS:
    bake_block(block_name, gltf_name, scene, camera)
print(f"Baked {len(BLOCKS)} HelioV face atlases to {OUTPUT}")
