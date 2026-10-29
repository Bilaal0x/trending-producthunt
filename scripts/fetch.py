"""Fetches the top Product Hunt launches and their images for the video.

    python scripts/fetch.py                  # yesterday's top 5 featured launches (Pacific time)
    python scripts/fetch.py --today          # what is trending today so far
    python scripts/fetch.py --date 2026-10-27

Credentials come from the environment or `.env` in the repository root: either
PRODUCT_HUNT_TOKEN (a developer token) or PRODUCT_HUNT_API_KEY + PRODUCT_HUNT_API_SECRET
(exchanged for a token with the client-credentials grant).

Writes `data/products.json` and the logos and gallery images to `data/media/`, which the video
reads at runtime, so no rebuild is needed for a new day.
"""
import argparse
import datetime as dt
import json
import os
import re
import shutil
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from zoneinfo import ZoneInfo

ROOT = Path(__file__).resolve().parent.parent
DATA = ROOT / "data"
MEDIA = DATA / "media"
API = "https://api.producthunt.com/v2/api/graphql"
TOKEN_URL = "https://api.producthunt.com/v2/oauth/token"
PACIFIC = ZoneInfo("America/Los_Angeles")
UA = "product-hunt-today-fframes/1.0"
TOP = 5
SHOTS = 3

QUERY = """
query($after: DateTime!, $before: DateTime!, $featured: Boolean, $first: Int!) {
  posts(first: $first, order: VOTES, featured: $featured, postedAfter: $after, postedBefore: $before) {
    edges { node {
      name tagline votesCount commentsCount url
      thumbnail { url }
      media { type url }
      topics(first: 3) { edges { node { name } } }
    } }
  }
}
"""


def load_env():
    env_file = ROOT / ".env"
    if env_file.exists():
        for line in env_file.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if line and not line.startswith("#") and "=" in line:
                key, value = line.split("=", 1)
                os.environ.setdefault(key.strip(), value.strip().strip('"').strip("'"))


def post_json(url, body, token=None):
    headers = {"Content-Type": "application/json", "Accept": "application/json", "User-Agent": UA}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    req = urllib.request.Request(url, json.dumps(body).encode(), headers)
    with urllib.request.urlopen(req, timeout=30) as res:
        return json.load(res)


def get_token():
    if os.environ.get("PRODUCT_HUNT_TOKEN"):
        return os.environ["PRODUCT_HUNT_TOKEN"]
    key, secret = os.environ.get("PRODUCT_HUNT_API_KEY"), os.environ.get("PRODUCT_HUNT_API_SECRET")
    if not (key and secret):
        sys.exit("Set PRODUCT_HUNT_TOKEN, or PRODUCT_HUNT_API_KEY and PRODUCT_HUNT_API_SECRET (in .env)")
    res = post_json(TOKEN_URL, {"client_id": key, "client_secret": secret, "grant_type": "client_credentials"})
    if "access_token" not in res:
        sys.exit(f"Product Hunt refused the API key/secret: {res}")
    return res["access_token"]


def fetch_posts(token, day, featured):
    start = dt.datetime.combine(day, dt.time(), PACIFIC)
    end = start + dt.timedelta(days=1)
    res = post_json(API, {"query": QUERY, "variables": {
        "after": start.isoformat(), "before": end.isoformat(), "featured": featured, "first": TOP,
    }}, token)
    if res.get("errors"):
        sys.exit(f"Product Hunt API error: {res['errors']}")
    return [edge["node"] for edge in res["data"]["posts"]["edges"]]


# Emoji and pictographs are not in the video fonts; drop them instead of drawing boxes.
EMOJI = re.compile("[\U0001F000-\U0001FAFF☀-➿⬀-⯿️‍⃣]")


def clean(text):
    text = EMOJI.sub("", text or "")
    return re.sub(r"\s+", " ", text).strip()


def imgix(url, **params):
    """Product Hunt serves images through imgix: ask for the size and a format fframes decodes."""
    parts = urllib.parse.urlsplit(url)
    return urllib.parse.urlunsplit(parts._replace(query=urllib.parse.urlencode(params)))


def download(url, dest):
    for attempt in range(3):
        try:
            req = urllib.request.Request(url, headers={"User-Agent": UA})
            with urllib.request.urlopen(req, timeout=60) as res, open(dest, "wb") as out:
                shutil.copyfileobj(res, out)
            return True
        except (urllib.error.URLError, TimeoutError) as err:
            print(f"  retry {attempt + 1} {url}: {err}", file=sys.stderr)
            time.sleep(1 + attempt)
    return False


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--today", action="store_true", help="today's launches so far, featured or not")
    ap.add_argument("--date", help="a Pacific-time day, YYYY-MM-DD (default: yesterday)")
    args = ap.parse_args()

    load_env()
    token = get_token()
    now = dt.datetime.now(PACIFIC).date()
    if args.today:
        day, mode = now, "today"
        posts = fetch_posts(token, day, None)
        if not posts:  # just after midnight nothing has launched yet
            day, mode = now - dt.timedelta(days=1), "daily"
            posts = fetch_posts(token, day, True)
    else:
        day = dt.date.fromisoformat(args.date) if args.date else now - dt.timedelta(days=1)
        mode = "daily"
        posts = fetch_posts(token, day, True)
    if not posts:
        sys.exit(f"No launches found for {day}")

    shutil.rmtree(MEDIA, ignore_errors=True)
    MEDIA.mkdir(parents=True)
    products = []
    for rank, post in enumerate(posts, 1):
        print(f"#{rank} {post['name']} ({post['votesCount']} votes)")
        logo = f"logo{rank}.png"
        if not (post.get("thumbnail") and download(
                imgix(post["thumbnail"]["url"], w=320, h=320, fit="crop", fm="png"), MEDIA / logo)):
            logo = None
        # Gallery images, then video posters if a launch has fewer than three images.
        media = post.get("media") or []
        sources = [m["url"] for m in media if m["type"] == "image"] + \
                  [m["url"] for m in media if m["type"] != "image" and m.get("url")]
        shots = []
        for url in sources[:SHOTS + 2]:
            name = f"shot{rank}_{len(shots)}.jpg"
            # Exactly 1600x960 (5:3, the gallery frame) without cropping: imgix pads other aspect
            # ratios with a blurred copy. Drawn ~1000 px wide, so it stays sharp.
            if download(imgix(url, w=1600, h=960, fit="fill", fill="blur", fm="jpg", q=92), MEDIA / name):
                shots.append(name)
            if len(shots) == SHOTS:
                break
        products.append({
            "rank": rank,
            "name": clean(post["name"]),
            "tagline": clean(post["tagline"]),
            "votes": post["votesCount"],
            "comments": post["commentsCount"],
            "topics": [clean(e["node"]["name"]) for e in post["topics"]["edges"]],
            "url": post["url"].split("?")[0],
            "logo": logo,
            "shots": shots,
        })

    result = {
        "mode": mode,
        "date": day.isoformat(),
        "date_label": f"{day:%A}, {day:%B} {day.day}",
        "fetched_at": dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds"),
        "products": products,
    }
    (DATA / "products.json").write_text(json.dumps(result, indent=2, ensure_ascii=False), encoding="utf-8")
    print(f"wrote {DATA / 'products.json'} ({len(products)} launches, {day})")


if __name__ == "__main__":
    main()
