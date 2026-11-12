"""
Django settings for the DevKit catalog API.

Phase 2 scope: published catalog data model, publication workflow, and the
public read API from docs/api/catalog-api.md. Publication/admin stays in
Django admin with authentication; public reads are unauthenticated.
"""

import os
from pathlib import Path

BASE_DIR = Path(__file__).resolve().parent.parent

# SECURITY WARNING: keep the secret key used in production secret!
# This placeholder is for local development only. Production deployments
# must inject a real key via environment variables or a secrets manager.
SECRET_KEY = "django-insecure-dev-only-not-for-production"

# SECURITY WARNING: don't run with debug turned on in production!
# Local development: set DEVKIT_DEBUG=true.
DEBUG = os.environ.get("DEVKIT_DEBUG", "false").lower() == "true"

# Production hosts come from the environment, comma-separated.
ALLOWED_HOSTS: list[str] = [
    h for h in os.environ.get("DEVKIT_ALLOWED_HOSTS", "").split(",") if h
]

INSTALLED_APPS = [
    "django.contrib.admin",
    "django.contrib.auth",
    "django.contrib.contenttypes",
    "django.contrib.sessions",
    "django.contrib.messages",
    "django.contrib.staticfiles",
    "rest_framework",
    "catalog",
]

MIDDLEWARE = [
    "django.middleware.security.SecurityMiddleware",
    "django.contrib.sessions.middleware.SessionMiddleware",
    "django.middleware.common.CommonMiddleware",
    "django.middleware.csrf.CsrfViewMiddleware",
    "django.contrib.auth.middleware.AuthenticationMiddleware",
    "django.contrib.messages.middleware.MessageMiddleware",
    "django.middleware.clickjacking.XFrameOptionsMiddleware",
    "catalog.middleware.ETagCacheMiddleware",
]

ROOT_URLCONF = "catalog_api.urls"

TEMPLATES = [
    {
        "BACKEND": "django.template.backends.django.DjangoTemplates",
        "DIRS": [],
        "APP_DIRS": True,
        "OPTIONS": {
            "context_processors": [
                "django.template.context_processors.request",
                "django.contrib.auth.context_processors.auth",
                "django.contrib.messages.context_processors.messages",
            ],
        },
    },
]

WSGI_APPLICATION = "catalog_api.wsgi.application"

# Local dev/test uses SQLite so tests run without services.
# PostgreSQL for deployment via environment variables (ADR 0001);
# set DEVKIT_DB=postgres and the POSTGRES_* variables to switch engines.
if os.environ.get("DEVKIT_DB") == "postgres":
    DATABASES = {
        "default": {
            "ENGINE": "django.db.backends.postgresql",
            "NAME": os.environ.get("POSTGRES_DB", "devkit_catalog"),
            "USER": os.environ.get("POSTGRES_USER", "devkit"),
            "PASSWORD": os.environ.get("POSTGRES_PASSWORD", ""),
            "HOST": os.environ.get("POSTGRES_HOST", "localhost"),
            "PORT": os.environ.get("POSTGRES_PORT", "5432"),
            "CONN_MAX_AGE": 60,
        }
    }
else:
    DATABASES = {
        "default": {
            "ENGINE": "django.db.backends.sqlite3",
            "NAME": BASE_DIR / "db.sqlite3",
        }
    }

LANGUAGE_CODE = "en-us"
TIME_ZONE = "UTC"
USE_I18N = True
USE_TZ = True

STATIC_URL = "static/"

DEFAULT_AUTO_FIELD = "django.db.models.BigAutoField"

# Catalog API conventions (docs/api/catalog-api.md).
REST_FRAMEWORK = {
    "DEFAULT_RENDERER_CLASSES": [
        "rest_framework.renderers.JSONRenderer",
    ],
    "DEFAULT_PAGINATION_CLASS": "catalog.middleware.StandardPagination",
    "PAGE_SIZE": 30,
    "DEFAULT_AUTHENTICATION_CLASSES": [
        "rest_framework.authentication.SessionAuthentication",
    ],
    "EXCEPTION_HANDLER": "catalog.middleware.error_envelope_handler",
}

# UTC ISO-8601 timestamps on every API response.
DATETIME_FORMAT = "iso-8601"
