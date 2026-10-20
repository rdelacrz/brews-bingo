"""Brand-token contrast and self-hosted typography regressions."""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[3]
THEME = ROOT / 'shared/ui/assets/theme.css'
PUBLIC = ROOT / 'platform/web/public'


def luminance(hex_color):
    rgb = [int(hex_color[i:i + 2], 16) / 255 for i in (1, 3, 5)]
    linear = [v / 12.92 if v <= .04045 else ((v + .055) / 1.055) ** 2.4 for v in rgb]
    return sum(v * weight for v, weight in zip(linear, (.2126, .7152, .0722)))


def contrast(a, b):
    lighter, darker = sorted((luminance(a), luminance(b)), reverse=True)
    return (lighter + .05) / (darker + .05)


class BrandThemeTests(unittest.TestCase):
    def setUp(self):
        self.css = THEME.read_text()
        self.colors = dict(re.findall(r'--([\w-]+):\s*(#[0-9a-f]{6});', self.css))

    def test_official_palette(self):
        self.assertEqual({k: self.colors[k] for k in ('paper', 'ink', 'accent', 'night')}, {
            'paper': '#f6f4ef', 'ink': '#210c03', 'accent': '#edad38', 'night': '#0e2a33',
        })

    def test_text_pairs_meet_normal_text_contrast(self):
        pairs = [('ink', 'paper'), ('ink', 'surface'), ('muted', 'paper'),
                 ('muted', 'surface'), ('accent-ink', 'paper'), ('accent-ink', 'soft'),
                 ('ink', 'accent'), ('ink', 'accent-hover'), ('paper', 'ink'),
                 ('accent', 'ink'), ('accent', 'night'), ('line', 'night')]
        for foreground, background in pairs:
            with self.subTest(foreground=foreground, background=background):
                self.assertGreaterEqual(contrast(self.colors[foreground], self.colors[background]), 4.5)

    def test_control_boundaries_and_focus_have_contrast(self):
        for foreground, background in [('control-line', 'surface'), ('control-line', 'paper'),
                                       ('accent-ink', 'paper'), ('accent', 'ink')]:
            with self.subTest(foreground=foreground, background=background):
                self.assertGreaterEqual(contrast(self.colors[foreground], self.colors[background]), 3)

    def test_all_status_tones_have_accessible_text_contrast(self):
        for tone in ('neutral', 'info', 'warning', 'success', 'danger'):
            with self.subTest(tone=tone):
                match = re.search(r'\.status-' + tone + r'\s*\{([^}]+)\}', self.css)
                assert match is not None, f'Missing status tone: {tone}'
                block = match[1]

                def color(property_name):
                    match = re.search(r'(?<![\w-])' + property_name + r':\s*([^;]+);', block)
                    assert match is not None, f'Missing {property_name}: {tone}'
                    value = match[1]
                    return self.colors[value[6:-1]] if value.startswith('var(--') else value
                self.assertGreaterEqual(contrast(color('color'), color('background')), 4.5)

    def test_fonts_are_local_woff2_and_licensed(self):
        urls = re.findall(r'url\("([^"]+)"\)', self.css)
        self.assertEqual(len(urls), 2)
        for url in urls:
            self.assertTrue(url.startswith('/assets/fonts/'))
            self.assertEqual((PUBLIC / url.lstrip('/')).read_bytes()[:4], b'wOF2')
        for name in ('Cormorant-Garamond-OFL.txt', 'DM-Sans-OFL.txt'):
            license_text = (PUBLIC / 'assets/fonts' / name).read_text()
            self.assertIn('SIL OPEN FONT LICENSE Version 1.1', license_text)
            self.assertIn('Copyright', license_text)
        self.assertNotIn('https://', self.css)
        self.assertEqual(self.css.count('font-display: swap;'), 2)


if __name__ == '__main__':
    unittest.main()
