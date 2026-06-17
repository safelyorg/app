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
    'mobile_phones',
    'tablets',
    'laptops',
    'computers',
    'accessories',
    'cameras',
    'tv_and_video',
    'audio',
    'gaming',
    'appliances',
    'furniture',
    'clothing',
    'shoes',
    'watches',
    'jewellery',
    'vehicles',
    'vehicle_parts',
    'property',
    'books',
    'sports',
    'toys',
    'tools',
    'services',
    'other'
);

CREATE TYPE report_types as ENUM (
    'scam',
    'fake_item',
    'no_delivery'
)
