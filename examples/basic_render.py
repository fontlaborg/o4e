#!/usr/bin/env python3
# this_file: examples/basic_render.py
"""Basic rendering example for o4e."""

import sys

try:
    import o4e
except ImportError:
    print("Error: o4e not installed. Install with: pip install o4e")
    sys.exit(1)


def main():
    """Demonstrate basic text rendering."""
    # Create renderer with auto backend selection
    renderer = o4e.TextRenderer()

    print(f"Using backend: {renderer.backend}")
    print(f"o4e version: {o4e.__version__}")

    # Render simple text
    font = o4e.Font("Arial", 48.0)

    texts = [
        "Hello World!",
        "Привет мир",  # Russian
        "Γειά σου κόσμε",  # Greek
        "مرحبا بالعالم",  # Arabic
        "你好世界",  # Chinese
    ]

    print("\nRendering test texts...")
    for text in texts:
        try:
            result = renderer.render(text, font, format="raw")
            if result:
                print(f"  ✓ {text[:20]}... -> {len(result)} bytes")
            else:
                print(f"  ✗ {text[:20]}... -> No output")
        except Exception as e:
            print(f"  ✗ {text[:20]}... -> Error: {e}")

    print("\n✓ Basic rendering test complete")


if __name__ == "__main__":
    main()
