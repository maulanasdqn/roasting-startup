# Roasting Startup Indonesia

A web application that generates brutal roasts for Indonesian startups using AI. Built with Rust, Leptos, and Axum.

## Features

- **AI-Powered Roasts**: Enter a startup URL and receive a brutal roast in Bahasa Indonesia
- **Google SSO**: Login with Google to save and vote on roasts
- **Voting System**: Upvote your favorite roasts with fire votes
- **Leaderboard**: See the most popular roasts ranked by fire count
- **Responsive Design**: Works on desktop and mobile devices

## Tech Stack

- **Frontend**: Leptos 0.8 (Rust WASM framework)
- **Backend**: Axum 0.8 (Rust web framework)
- **Database**: PostgreSQL with SeaORM
- **Authentication**: Google OAuth 2.0 with PKCE
- **AI**: OpenRouter API (supports multiple LLM providers)
- **Styling**: SCSS with BEM methodology
- **Build**: Nix flakes for reproducible builds

## Prerequisites

- Nix with flakes enabled
- PostgreSQL database
- Google OAuth credentials
- OpenRouter API key

## Environment Variables

Create a `.env` file based on `.env.example`:

```bash
# Database
DATABASE_URL=postgres://user:password@localhost:5432/roasting_startup

# Google OAuth2
GOOGLE_CLIENT_ID=your-client-id.apps.googleusercontent.com
GOOGLE_CLIENT_SECRET=your-client-secret
GOOGLE_REDIRECT_URI=http://localhost:3000/auth/callback

# AI Provider
OPENROUTER_API_KEY=sk-or-v1-your-api-key

# Optional: Use local LLM instead of OpenRouter
# USE_LOCAL_LLM=1
```

## Database Setup

1. Create a PostgreSQL database:

```bash
createdb roasting_startup
```

2. The application automatically runs migrations on startup.

## Development

### Using Nix (Recommended)

Enter the development shell:

```bash
nix develop
```

Build and run:

```bash
nix build .#default
./result/bin/roasting-api
```

### Manual Setup

If not using Nix, ensure you have:
- Rust nightly toolchain
- wasm32-unknown-unknown target
- sass compiler

## Project Structure

```
roasting-startup/
├── migrations/           # SQL migrations
├── roasting-api/         # Axum server binary
│   └── src/main.rs       # Routes, handlers, SSR shell
├── roasting-app/         # Core business logic
│   └── src/
│       ├── domain/       # Domain models (User, Roast, Vote)
│       ├── application/  # Use cases (GenerateRoast)
│       └── infrastructure/
│           ├── auth/     # Google OAuth
│           ├── db/       # SeaORM repositories
│           ├── openrouter/  # AI API client
│           ├── scraper/  # Website scraper
│           └── security/ # Rate limiting, input validation
├── roasting-ui/          # Leptos frontend components
│   └── src/
│       ├── components/   # Reusable UI components
│       └── pages/        # Page components
├── roasting-errors/      # Error types
├── style/                # SCSS stylesheets
├── public/               # Static assets
└── flake.nix             # Nix build configuration
```

## API Endpoints

| Endpoint | Method | Auth | Description |
|----------|--------|------|-------------|
| `/` | GET | No | Home page |
| `/auth/login` | GET | No | Initiate Google OAuth |
| `/auth/callback` | GET | No | OAuth callback |
| `/auth/logout` | POST | Yes | Logout |
| `/auth/me` | GET | No | Get current user |
| `/roast` | POST | No | Generate a roast |
| `/r/{id}` | GET | No | View a roast |
| `/leaderboard` | GET | No | Leaderboard page |
| `/api/roast/{id}/vote` | POST | Yes | Toggle vote |
| `/api/leaderboard` | GET | No | Leaderboard JSON |

## Security Features

- **Rate Limiting**: 5 requests/minute, 20 requests/hour per IP
- **Daily Cost Limit**: Maximum 100 AI requests per day
- **Input Validation**: URL sanitization and validation
- **CSRF Protection**: State parameter in OAuth flow
- **PKCE**: Proof Key for Code Exchange for OAuth
- **Session Security**: HTTP-only cookies with SameSite=Lax

## Configuration

### Rate Limits

Rate limits are configured in `roasting-app/src/infrastructure/security/rate_limiter.rs`:

- Per-minute limit: 5 requests
- Per-hour limit: 20 requests

### Cost Tracking

Daily request limit is configured in `roasting-app/src/infrastructure/security/cost_tracker.rs`:

- Default: 100 requests per day

## Deployment

### Using Nix

Build the production binary:

```bash
nix build .#default
```

The output includes:
- `result/bin/roasting-api` - Server binary
- `result/site/` - Static assets (WASM, CSS, JS)

### Environment

Required environment variables for production:
- Set `LEPTOS_SITE_ROOT` to the path containing static assets
- Use a persistent session store (current implementation uses in-memory store)
- Set secure cookie options for HTTPS

## License

MIT

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Submit a pull request
