"""Bounded streaming fingerprints for explicit local development evidence."""
import hashlib


def fingerprint(path, maximum):
    if type(maximum) is not int or maximum < 0:
        raise ValueError("invalid fingerprint byte limit")
    digest=hashlib.sha256()
    total=0
    with path.open("rb") as stream:
        while True:
            block=stream.read(min(1024*1024,maximum-total+1))
            if not block:
                return digest.hexdigest()
            total+=len(block)
            if total>maximum:
                raise ValueError("fingerprint input exceeds byte limit")
            digest.update(block)
