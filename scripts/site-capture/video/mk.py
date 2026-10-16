#!/usr/bin/env python3
# mk.py <body.js> <out.webm> <lead 1|0> [KEY=VALUE ...]: builds a run-code script from prelude.js and a
# body file, and prints its path. Env: OUT (default ./out), DEMO_PORT (default 8090).
import os, sys, pathlib
here = pathlib.Path(__file__).parent
out_dir = pathlib.Path(os.environ.get('OUT', './out')).resolve()
port = os.environ.get('DEMO_PORT', '8090')
body, out, lead = sys.argv[1:4]
out_file = out_dir / out
src = (here / 'prelude.js').read_text().replace('__OUT__', str(out_file)).replace('__LEAD__', 'true' if lead == '1' else 'false')
code = (here / body).read_text().replace('localhost:8090', 'localhost:' + port)
for kv in sys.argv[4:]:
    k, v = kv.split('=', 1)
    code = code.replace('__' + k + '__', v)
src = src.replace('  // __BODY__', code)
out_dir.mkdir(parents=True, exist_ok=True)
dst = out_dir / ('run-' + pathlib.Path(out).stem + '.js')
dst.write_text(src)
print(dst)
