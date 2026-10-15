import io
from pathlib import Path
import sys
import tempfile
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from provision_code_models import checked_url, write_verified

class ProvisionTests(unittest.TestCase):
    def test_redirect_admission_rejects_arbitrary_http_credentials_and_ports(self):
        checked_url('https://huggingface.co/a/model')
        checked_url('https://cas-bridge.xethub.hf.co/public/asset?signature=value')
        checked_url('https://cas-bridge.xethub-eu.hf.co/public/asset?signature=value')
        for url in ['http://huggingface.co/a','https://example.com/model','https://huggingface.co.evil.test/a','https://u:p@huggingface.co/a','https://huggingface.co:8443/a','file:///tmp/model']:
            with self.subTest(url=url),self.assertRaises(ValueError): checked_url(url)
    def test_corrupt_or_oversized_input_never_replaces_existing_cache_asset(self):
        import hashlib
        asset={'bytes':5,'sha256':hashlib.sha256(b'model').hexdigest()}
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory); path=root/'model.onnx'; path.write_bytes(b'prior')
            for data in [b'other',b'model extra',b'mod']:
                with self.assertRaises(ValueError): write_verified(io.BytesIO(data),path,asset,root)
                self.assertEqual(path.read_bytes(),b'prior')
            write_verified(io.BytesIO(b'model'),path,asset,root)
            self.assertEqual(path.read_bytes(),b'model')
            self.assertEqual(sorted(p.name for p in root.iterdir()),['model.onnx'])
    def test_destination_symlink_escape_is_rejected(self):
        import hashlib
        with tempfile.TemporaryDirectory() as directory,tempfile.TemporaryDirectory() as outside:
            root=Path(directory); target=Path(outside)/'model.onnx'; target.write_bytes(b'prior')
            path=root/'model.onnx'; path.symlink_to(target)
            with self.assertRaises(ValueError): write_verified(io.BytesIO(b'model'),path,{'bytes':5,'sha256':hashlib.sha256(b'model').hexdigest()},root)
            self.assertEqual(target.read_bytes(),b'prior')

    @unittest.skipUnless(hasattr(__import__('os'),'mkfifo'),'POSIX FIFO fixture')
    def test_fifo_cache_source_is_rejected_in_a_bounded_child(self):
        import os,subprocess
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory); os.mkfifo(root/'model.onnx')
            code="from pathlib import Path; import sys; sys.path.insert(0,sys.argv[2]); from provision_code_models import open_regular; root=Path(sys.argv[1]);\ntry: open_regular(root/'model.onnx',root)\nexcept ValueError: sys.exit(0)\nraise AssertionError('FIFO admitted')"
            result=subprocess.run([sys.executable,'-c',code,str(root),str(Path(__file__).resolve().parents[1])],capture_output=True,timeout=2)
            self.assertEqual(result.returncode,0,result.stderr.decode())

if __name__=='__main__':unittest.main()
