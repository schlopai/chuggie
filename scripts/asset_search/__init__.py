"""Text + image search over the Ninja Adventure asset catalog.

    from asset_search import AssetSearch
    s = AssetSearch()
    s.search_text("cozy wooden interior floor")
    s.search_image("path/to/tile.png")
    s.match_tilemap("path/to/screenshot.png", tile_size=16)
"""
from asset_search.search import AssetSearch  # noqa: F401

__all__ = ["AssetSearch"]
