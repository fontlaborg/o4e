#!/usr/bin/env python3
# this_file: examples/test_png_output.py
"""PNG output test for o4e."""

import sys
from pathlib import Path

try:
    import o4e
except ImportError:
    print("Error: o4e not installed. Install with: pip install o4e")
    sys.exit(1)


def main():
    """Test PNG rendering and save to file."""
    # Create renderer
    renderer = o4e.TextRenderer()

    print(f"Using backend: {renderer.backend}")

    # Create font
    font = o4e.Font("Arial", 64.0)

    # Render to PNG
    print("\nRendering 'Hello, World!' to PNG...")
    try:
        png_data = renderer.render("Hello, World!", font, format="png")

        if png_data:
            # Save to file (we're already in the examples directory)
            output_path = Path("hello.png")
            output_path.write_bytes(png_data)
            print(f"  ✓ Saved PNG to {output_path} ({len(png_data)} bytes)")
        else:
            print("  ✗ No PNG data returned")
            sys.exit(1)
    except Exception as e:
        print(f"  ✗ Error: {e}")
        sys.exit(1)

    print("\n✓ PNG output test complete")


if __name__ == "__main__":
    main()
