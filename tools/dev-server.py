"""开发用静态服务器：比 python -m http.server 多一件事——禁用缓存。

python 自带的 http.server 不发 Cache-Control，WebView 会按 Last-Modified 做
启发式缓存，改了 js 刷新页面仍可能拿到旧文件，看起来像"改了没生效"。
这里统一加 no-store，配合 shell.js 里的自动刷新（HEAD 读 Last-Modified）。

用法：
    python tools/dev-server.py 1755 src
"""

import functools
import http.server
import socketserver
import sys


class Handler(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header("Cache-Control", "no-store, must-revalidate")
        self.send_header("Pragma", "no-cache")
        self.send_header("Expires", "0")
        super().end_headers()

    def log_message(self, fmt, *args):
        # 每 900ms 会来一轮 HEAD，日志太吵，只留错误
        if args and str(args[1]).startswith("4"):
            super().log_message(fmt, *args)


def main():
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 1755
    directory = sys.argv[2] if len(sys.argv) > 2 else "src"
    socketserver.TCPServer.allow_reuse_address = True
    handler = functools.partial(Handler, directory=directory)
    with socketserver.ThreadingTCPServer(("127.0.0.1", port), handler) as httpd:
        print(f"serving {directory} on http://127.0.0.1:{port} (no-store)")
        httpd.serve_forever()


if __name__ == "__main__":
    main()
