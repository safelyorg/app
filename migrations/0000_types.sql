CREATE TYPE seller_location AS ENUM (
    'islamabad',
    'lahore',
    'quetta',
    'peshawar'
);

CREATE TYPE seller_verification AS ENUM (
    'verified',
    'flagged',
    'unknown'
);

CREATE TYPE listing_categories AS ENUM (

)

CREATE TYPE report_types as ENUM (
    'scam',
    'fake_item',
    'no_delivery'
)
