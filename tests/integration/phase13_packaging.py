from pathlib import Path
import unittest

ROOT=Path(__file__).resolve().parent
class PriorityPackaging(unittest.TestCase):
 def test_background_dropins_are_shipped(self):
  repo=Path.cwd();build=(repo/'packaging/debian/build-packages.sh').read_text()
  for unit in ['filedock-core','filedock-s3','filedock-rclone']:
   text=(repo/f'packaging/systemd/{unit}.service.d/background-priority.conf').read_text()
   self.assertIn('Nice=10',text);self.assertIn('IOSchedulingClass=idle',text)
  self.assertIn('filedock-core.service.d/background-priority.conf',build)
  self.assertIn('for fd_connector in filedock-s3 filedock-rclone',build)
  installer=(repo/'packaging/installer/install.sh').read_text()
  self.assertIn('systemctl try-restart filedock-s3.service filedock-rclone.service',installer)
  for unit in ['smbd','nfs-server','docker']:
   self.assertFalse((repo/f'packaging/systemd/{unit}.service.d/background-priority.conf').exists())
if __name__=='__main__':unittest.main()
