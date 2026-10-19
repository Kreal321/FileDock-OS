#!/usr/bin/env python3
"""No-network checks for optional S3 trace mode and default fault-proxy semantics."""
import importlib.util
import os
from pathlib import Path
import unittest
from unittest.mock import patch


class ProxyModes(unittest.TestCase):
    def test_condition_removal_is_default_and_preservation_is_explicit(self):
        path=Path(__file__).resolve().parents[1]/'fixtures/s3-no-conditional/proxy.py'
        with patch.dict(os.environ,{'FD_S3_ACCESS_KEY':'synthetic','FD_S3_SECRET_KEY':'synthetic','FD_S3_PRESERVE_CONDITIONS':'0','FD_S3_TRACE_PROCESS':'0'}):
            spec=importlib.util.spec_from_file_location('phase12_proxy',path)
            proxy=importlib.util.module_from_spec(spec);spec.loader.exec_module(proxy)
        _,headers=proxy.sign('PUT','/bucket/key',{'If-Match':'etag','If-None-Match':'*'},b'fixture')
        self.assertNotIn('if-match',headers)
        self.assertNotIn('if-none-match',headers)
        self.assertFalse(proxy.TRACE_PROCESS)
        proxy.PRESERVE_CONDITIONS=True
        _,headers=proxy.sign('PUT','/bucket/key',{'If-Match':'etag'},b'fixture')
        self.assertEqual(headers['if-match'],'etag')
        self.assertIn('if-match',headers['authorization'])


if __name__=='__main__':unittest.main()
