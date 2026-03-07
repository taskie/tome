# tome-web

Next.js web frontend for [tome](../README.md).

Provides a browser UI for browsing repositories, snapshots, diffs, file history, blobs, stores, machines, and tags managed by `tome-server`.

## Requirements

- Node.js 20.9+
- A running `tome-server` instance

## Setup

```bash
cp env.local.example .env.local
# Edit .env.local and set TOME_API_URL if tome-server is not at http://localhost:8080
```

## Development

```bash
npm install
npm run dev
```

Open [http://localhost:3000](http://localhost:3000).

## Production

```bash
npm run build
npm start
```

## Configuration

| Environment variable | Description                       | Default                 |
| -------------------- | --------------------------------- | ----------------------- |
| `TOME_API_URL`       | URL of the `tome-server` instance | `http://localhost:8080` |

`TOME_API_URL` is a server-side variable (no `NEXT_PUBLIC_` prefix needed). All API calls are made server-side, so no CORS configuration is required.

## Pages

| Path                           | Description                    |
| ------------------------------ | ------------------------------ |
| `/`                            | Repository list                |
| `/repositories/[name]`         | Snapshot list for a repository |
| `/repositories/[name]/diff`    | Diff between two snapshots     |
| `/repositories/[name]/files`   | Current tracked files          |
| `/repositories/[name]/history` | File change history            |
| `/snapshots/[id]`              | Snapshot entries               |
| `/blobs/[digest]`              | Blob details and occurrences   |
| `/diff`                        | Cross-repository diff          |
| `/stores`                      | Store list                     |
| `/machines`                    | Machine list                   |
| `/tags`                        | Tag list                       |
| `/sync-peers`                  | Sync peer list                 |

## License

Apache License 2.0
