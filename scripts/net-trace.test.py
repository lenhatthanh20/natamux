#!/usr/bin/env python3
"""Tests of net-trace.py. Run: python3 scripts/net-trace.test.py"""

import importlib.util
import pathlib
import unittest

SPEC = importlib.util.spec_from_file_location("net_trace", pathlib.Path(__file__).with_name("net-trace.py"))
net_trace = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(net_trace)

LOCAL = '4242 connect(9, {sa_family=AF_INET, sin_port=htons(4317), sin_addr=inet_addr("127.0.0.1")}, 16) = 0'
LOCAL6 = ('4242 connect(9, {sa_family=AF_INET6, sin6_port=htons(4317), sin6_flowinfo=htonl(0), '
          'inet_pton(AF_INET6, "::1", &sin6_addr), sin6_scope_id=0}, 28) = 0')
MAPPED = LOCAL6.replace('"::1"', '"::ffff:127.0.0.1"')
UNIX = '4242 connect(5, {sa_family=AF_UNIX, sun_path="/tmp/.X11-unix/X99"}, 20) = 0'
NETLINK = '4242 sendto(6, [{nlmsg_len=20}], 20, 0, {sa_family=AF_NETLINK, nl_pid=0, nl_groups=00000000}, 12) = 20'
REMOTE = '4243 connect(11, {sa_family=AF_INET, sin_port=htons(443), sin_addr=inet_addr("140.82.112.3")}, 16) = -1 EINPROGRESS'
REMOTE6 = LOCAL6.replace('"::1"', '"2606:50c0:8000::154"').replace("4317", "443")
DNS = '4243 connect(12, {sa_family=AF_INET, sin_port=htons(53), sin_addr=inet_addr("127.0.0.53")}, 16) = 0'
SENDMSG = ('4243 sendmsg(12, {msg_name={sa_family=AF_INET, sin_port=htons(123), sin_addr=inet_addr("10.0.0.7")}, '
           'msg_namelen=16, msg_iov=[{iov_base="x", iov_len=1}], msg_iovlen=1, msg_controllen=0, msg_flags=0}, 0) = 1')


class NetTraceTest(unittest.TestCase):
    def test_loopback_unix_and_netlink_pass(self):
        self.assertEqual(net_trace.check([LOCAL, LOCAL6, MAPPED, UNIX, NETLINK]), ([], 3))

    def test_another_address_fails(self):
        for line, address in ((REMOTE, "140.82.112.3"), (REMOTE6, "2606:50c0:8000::154"), (SENDMSG, "10.0.0.7")):
            problems, _ = net_trace.check([LOCAL, line])
            self.assertEqual(len(problems), 1, line)
            self.assertIn(address, problems[0])

    def test_a_dns_lookup_fails_even_on_a_loopback_resolver(self):
        problems, local = net_trace.check([DNS])
        self.assertEqual((len(problems), local), (1, 0))
        self.assertIn("DNS", problems[0])


if __name__ == "__main__":
    unittest.main()
