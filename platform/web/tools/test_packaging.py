import hashlib
import base64
from pathlib import Path
import tempfile
import unittest
import os
from build import protect_assets
from local_worker import configuration


class WebPackagingTests(unittest.TestCase):
    def test_assets_do_not_shadow_api_or_developer_boundary(self):
        config = configuration(Path('/not-deployed/static'))
        self.assertEqual(config['assets']['run_worker_first'], ['/api', '/api/*', '/_dev/*'])
        self.assertEqual(config['assets']['not_found_handling'], 'single-page-application')
        self.assertEqual({b['class_name'] for b in config['durable_objects']['bindings']}, {'AccountsObject', 'GameDirectoryObject', 'GameObject'})
        self.assertFalse(config['workers_dev'])
        self.assertFalse(config['preview_urls'])
        self.assertEqual(config['routes'], [])
        self.assertNotIn('vars', config)
        self.assertNotIn('env', config)
        self.assertNotIn('build', config)

    def test_csp_hashes_exact_loader_without_allowing_arbitrary_inline_script(self):
        with tempfile.TemporaryDirectory(dir=os.environ.get('TMPDIR', '/root/.hermes/cache/scratch')) as directory:
            root = Path(directory)
            script = '\nimport("/assets/app.js");\n'
            html = '<html><script type="module">' + script + '</script></html>'
            (root / 'index.html').write_text(html)
            self.assertEqual(protect_assets(root), hashlib.sha256(html.encode()).hexdigest())
            headers = (root / '_headers').read_text()
            expected = base64.b64encode(hashlib.sha256(script.encode()).digest()).decode()
            self.assertIn("'sha256-" + expected + "'", headers)
            self.assertNotIn("script-src 'unsafe-inline'", headers)
            self.assertIn("connect-src 'self'", headers)
            self.assertIn('Cache-Control: no-store', headers)
            self.assertIn('Referrer-Policy: no-referrer', headers)
            self.assertEqual((root / 'index.html').read_text(), html)


if __name__ == '__main__':
    unittest.main()
