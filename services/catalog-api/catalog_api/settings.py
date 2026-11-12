"""
Django settings for the DevKit catalog API.

Phase 0 scope: the service is a scaffold with a health endpoint and the
URL contract from docs/api/catalog-api.md. Publication models, PostgreSQL
configuration, and authentication arrive in Phase 2 (HANDOFF.md §17).
"""

from pathlib import Path

BASE_DIR = Path(__file__).resolve().parent.parent

# SECURITY WARNING: keep the secret key used in production secret!
# This placeholder is for local development only. Production deployments
# must inject a real key via environment variables or a secrets manager.
SECRET_KEY = "django-insecure-dev-only-not-for-production"

# SECURITY WARNING: don't run with debug turned on in production!
DEBUG = True

ALLOWED_HOSTS: list[str] = []

INSTALLED_APPS = [
    "django.contrib.contenttypes",
    "django.contrib.staticfiles",
    "rest_framework",
    "catalog",
]

MIDDLEWARE = [
    "django.middleware.security.SecurityMiddleware",
    "django.middleware.common.CommonMiddleware",
    "django.middleware.clickjacking.XFrameOptionsMiddleware",
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
            ],
        },
    },
]

WSGI_APPLICATION = "catalog_api.wsgi.application"

# Phase 0 uses SQLite for local development so tests run without services.
# Phase 2 switches to PostgreSQL for deployment (ADR 0001); configuration
# will be environment-driven at that point.
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
    "DEFAULT_PAGINATION_CLASS": None,  # endpoints opt in explicitly
    "EXCEPTION_HANDLER": None,  # endpoints use the documented error envelope
}

# UTC ISO-8601 timestamps on every API response.
DATETIME_FORMAT = "iso-8601"
