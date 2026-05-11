#!/usr/bin/env python3
from PIL import Image, ImageDraw
import os

def create_icon(size, output_path):
    # Create a nice blue gradient icon
    img = Image.new('RGBA', (size, size))
    draw = ImageDraw.Draw(img)
    
    # Blue gradient background (from user's icon colors)
    for y in range(size):
        t = y / size
        r = int(102 + (118 - 102) * t)
        g = int(126 + (75 - 126) * t)
        b = int(234 + (162 - 234) * t)
        draw.line([(0, y), (size, y)], fill=(r, g, b, 255))
    
    # Add a subtle white rounded rectangle in center for style
    margin = int(size * 0.15)
    radius = int(size * 0.15)
    draw.rounded_rectangle(
        [(margin, margin), (size - margin, size - margin)],
        radius=radius,
        fill=(255, 255, 255, 30)
    )
    
    # Save as PNG
    img.save(output_path, 'PNG')
    print(f'Created: {output_path}')

if __name__ == '__main__':
    output_dir = 'src-tauri/icons'
    os.makedirs(output_dir, exist_ok=True)
    
    # Generate required icon sizes
    icons = [
        (32, 32, '32x32.png'),
        (128, 128, '128x128.png'),
        (256, 256, '128x128@2x.png'),
        (512, 512, 'icon_512x512.png'),
    ]
    
    for w, h, filename in icons:
        output_path = os.path.join(output_dir, filename)
        create_icon(w, output_path)
    
    print(f'\nAll icons generated in: {output_dir}')
