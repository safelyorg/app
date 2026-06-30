(function () {
  "use strict";
  if (!window.__safelyAddTab) return;

  var root = window.__safelyRoot;
  var protectSubTab = "orders";
  var formStep = 1;
  var sellerStep = 1;
  var sellerMethod = "handle";

  var sellerTabHTML =
    '<div class="step-indicator-row" id="safely-sel-step-row">' +
    '<div class="safely-sel-step safely-sel-active" data-sstep="1"><div class="safely-sel-step-dot safely-sel-dot-active" id="safely-sel-dot-1">1</div>Lookup</div>' +
    '<div class="safely-sel-step-line"></div>' +
    '<div class="safely-sel-step" data-sstep="2"><div class="safely-sel-step-dot" id="safely-sel-dot-2">2</div>Review</div>' +
    '<div class="safely-sel-step-line"></div>' +
    '<div class="safely-sel-step" data-sstep="3"><div class="safely-sel-step-dot" id="safely-sel-dot-3">3</div>Ship</div>' +
    "</div>" +
    '<div class="sub sub-visible" id="safely-sel-lookup"><div class="safely-seller-lookup-empty"><div class="safely-seller-lookup-icon"><svg width="28" height="28" fill="none" stroke="#ffffff" stroke-width="1.8" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4"/></svg></div><div><h3 style="font-size:18px;font-weight:700;letter-spacing:-0.02em;color:#ffffff;margin:0">Look Up Order</h3><p style="font-size:13px;color:#a0a0a0;margin:6px 0 0;line-height:1.6">Enter the buyer\'s order number to view order details and manage fulfillment.</p></div><div class="safely-seller-lookup-form"><div><label class="safely-seller-lookup-label">Order Number</label><input type="text" placeholder="COV-XXXX-XXXX" class="field safely-field-mono"/></div><button type="button" class="btn btn-black" id="safely-sel-lookup-btn"><svg width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"/></svg>Look Up Order</button></div></div></div>' +
    '<div class="sub" id="safely-sel-order"><button type="button" class="back-link" id="safely-sel-back-lookup">\u2190 Back to Lookup</button><div class="safely-order-num">COV-A3BX-7KPQ</div><div class="card"><div class="safely-product-img-placeholder"><svg width="36" height="36" fill="none" stroke="#636366" stroke-width="1.2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z"/></svg><span class="safely-pill-status safely-s-paid">Paid</span></div><div class="card-inner"><div style="display:flex;justify-content:space-between;align-items:flex-start;margin-bottom:14px"><div><h3 style="font-size:17px;font-weight:700;color:#ffffff;margin:0">iPhone 15 Pro</h3><p class="mono" style="font-size:11px;color:#a0a0a0;margin:4px 0 0">https://olx.com/ad/123456</p></div></div><div class="brow"><span style="color:#a0a0a0">Product Amount</span><span>PKR 250,000</span></div><div class="brow"><span style="color:#a0a0a0">Delivery Charges</span><span>PKR 500</span></div><div class="brow brow-total"><span>You Receive</span><span>PKR 250,500</span></div></div></div><div class="card"><div class="card-inner"><div class="section-label" style="margin-bottom:12px">Buyer Payment Proof</div><div class="safely-proof-row"><span class="safely-proof-key">Buyer Name</span><span class="safely-proof-val">Ahmed Khan</span></div><div class="safely-proof-row"><span class="safely-proof-key">Method</span><span class="safely-proof-val">JazzCash</span></div><div class="safely-proof-row"><span class="safely-proof-key">Transaction ID</span><span class="safely-proof-val mono" style="font-size:12px">TXN-987654321</span></div><div class="safely-proof-row"><span class="safely-proof-key">Buyer Phone</span><span class="safely-proof-val">+92 300 1234567</span></div><div class="safely-proof-row"><span class="safely-proof-key">Delivery Address</span><span class="safely-proof-val" style="font-size:12px">House 12, G-9/2, Islamabad</span></div></div></div><div class="banner banner-blue">Buyer has submitted payment. Review the proof above and accept or reject the order.</div><div class="safely-seller-actions"><button type="button" class="btn btn-ghost" id="safely-sel-reject">Reject</button><button type="button" class="btn btn-black" id="safely-sel-accept">Accept Order</button></div></div>' +
    '<div class="sub" id="safely-sel-ship"><button type="button" class="back-link" id="safely-sel-back-order">\u2190 Back to Order</button><div><h3 class="safely-ship-header-title">Mark as Shipped</h3><p class="safely-ship-header-sub">COV-A3BX-7KPQ \u00b7 iPhone 15 Pro</p></div><div class="banner banner-green">Payout of <strong>PKR 250,500</strong> will be sent to your Easypaisa account ending in \u00b7\u00b7\u00b77890 after delivery is confirmed.</div><div class="card"><div class="card-inner" style="display:flex;flex-direction:column;gap:12px"><div><label class="field-label">Tracking ID *</label><input type="text" placeholder="e.g. TCS-123456789" class="field"/></div><div><label class="field-label">Courier Service</label><select class="field"><option>TCS</option><option>Leopards</option><option>M&P</option><option>DHL</option><option>FedEx</option><option>By Hand</option></select></div><div><label class="field-label">Handover Video (recommended)</label><label class="upload-zone" style="height:90px"><svg width="22" height="22" fill="none" stroke="#636366" stroke-width="1.5" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z"/></svg><span style="font-size:12px;color:#636366">Upload handover video \u00b7 max 50MB</span><input type="file" accept="video/*"/></label></div></div></div><div class="card" style="overflow:hidden"><div class="card-inner" style="display:flex;flex-direction:column;gap:16px"><div><div class="section-label" style="margin-bottom:5px">Delivery Confirmation QR</div><p class="safely-qr-desc">Print this QR and seal it on the parcel. The buyer scans it on delivery to instantly confirm receipt and release your payout.</p></div><div class="safely-qr-section"><div class="safely-qr-box"><svg width="90" height="90" fill="none" stroke="#000000" stroke-width="1.5" viewBox="0 0 24 24"><rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/><rect x="9" y="9" width="6" height="6" rx="0.5"/></svg></div><div style="text-align:center"><div class="safely-qr-order-id">COV-A3BX-7KPQ</div><div class="safely-qr-hint">Scan to confirm delivery \u00b7 safely.sh/verify</div></div></div><div class="safely-qr-actions"><button type="button" class="btn btn-black" style="font-size:13px;padding:11px 16px"><svg width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"/></svg>Download</button><button type="button" class="btn btn-ghost" style="font-size:13px;padding:11px 16px"><svg width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M17 17h2a2 2 0 002-2v-4a2 2 0 00-2-2H5a2 2 0 00-2 2v4a2 2 0 002 2h2m2 4h6a2 2 0 002-2v-4a2 2 0 00-2-2H9a2 2 0 00-2 2v4a2 2 0 002 2zm8-12V5a2 2 0 00-2-2H9a2 2 0 00-2 2v4h10z"/></svg>Print</button></div><p class="safely-qr-footer">Affix to the sealed flap of the parcel \u00b7 Do not cover with tape</p></div></div><button type="button" class="btn btn-black" id="safely-sel-confirm-ship"><svg width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>Confirm Shipped</button></div>' +
    '<div class="sub" id="safely-sel-shipped"><div class="safely-order-num">COV-A3BX-7KPQ</div><div class="card"><div class="safely-product-img-placeholder"><svg width="36" height="36" fill="none" stroke="#636366" stroke-width="1.2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z"/></svg><span class="safely-pill-status safely-s-shipped" style="font-size:10px">Shipped</span></div><div class="card-inner"><h3 style="font-size:17px;font-weight:700;color:#ffffff;margin:0 0 14px">iPhone 15 Pro</h3><div class="brow"><span style="color:#a0a0a0">Product Price</span><span>PKR 250,000</span></div><div class="brow"><span style="color:#a0a0a0">Delivery Charges</span><span>PKR 500</span></div><div class="brow brow-total"><span>You Receive</span><span>PKR 250,500</span></div></div></div><div class="card"><div class="card-inner"><div class="section-label" style="margin-bottom:12px">Shipment Details</div><div class="safely-proof-row"><span class="safely-proof-key">Tracking ID</span><span class="safely-proof-val mono" style="font-size:12px">TCS-123456789</span></div><div class="safely-proof-row"><span class="safely-proof-key">Courier</span><span class="safely-proof-val">TCS</span></div><div class="safely-proof-row"><span class="safely-proof-key">Shipped</span><span class="safely-proof-val">May 28 \u00b7 3:45 PM</span></div></div></div><div class="safely-waiting-card"><div class="safely-waiting-icon"><svg width="22" height="22" fill="none" stroke="#a0a0a0" stroke-width="1.8" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"/></svg></div><div><div class="safely-waiting-title">Waiting for Buyer</div><p class="safely-waiting-desc">The buyer needs to confirm delivery before<br/>your payout of <strong style="color:#ffffff">PKR 250,500</strong> is released.</p></div><div class="safely-payout-row"><span style="font-size:12px;color:#a0a0a0">Payout to</span><span style="font-size:12px;font-weight:600;color:#ffffff">Easypaisa \u00b7 \u00b7\u00b77890</span></div></div></div>';

  var protectTabHTML =
    '<div class="safely-sub-tabs" id="safely-protect-subs">' +
    '<button class="safely-sub-tab safely-active" data-sub="orders">Orders</button>' +
    '<button class="safely-sub-tab" data-sub="new-order">New Order</button>' +
    '<button class="safely-sub-tab" data-sub="seller">Seller</button>' +
    "</div>" +
    '<div id="safely-sub-orders"><div class="safely-orders-empty"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2"/><rect x="9" y="3" width="6" height="4" rx="1"/><path d="M9 14l2 2 4-4"/></svg><div class="safely-orders-empty-title">No active orders</div><div class="safely-orders-empty-sub">Your escrow orders will appear here</div></div></div>' +
    '<div id="safely-sub-new-order" style="display:none;">' +
    '<div class="step-indicator-row" id="safely-step-row"><div class="new-step step-active" data-step="1"><div class="new-step-dot dot-active" id="safely-dot-1">1</div>Details</div><div class="step-line" id="safely-line-1"></div><div class="new-step" data-step="2"><div class="new-step-dot" id="safely-dot-2">2</div>Payment</div><div class="step-line" id="safely-line-2"></div><div class="new-step" data-step="3"><div class="new-step-dot" id="safely-dot-3">3</div>Done</div></div>' +
    '<div class="sub sub-visible" id="safely-step-1"><div class="card"><div class="card-inner card-gap"><label class="img-upload-zone" style="cursor:pointer;display:flex;align-items:center;justify-content:space-between;gap:12px;padding:12px 14px;background:#0a0a0a;border:1.5px dashed #1a1a1a;border-radius:12px;transition:border-color 0.15s"><div style="display:flex;flex-direction:column;gap:3px"><span style="font-size:13px;font-weight:600;color:#ffffff">Product image</span><span style="font-size:11px;color:#a0a0a0">PNG or JPG, up to 5MB</span></div><div style="display:flex;flex-direction:column;align-items:center;gap:4px;flex-shrink:0"><div style="width:36px;height:36px;border-radius:8px;background:#1a1a1a;display:flex;align-items:center;justify-content:center"><svg width="16" height="16" fill="none" stroke="#a0a0a0" stroke-width="2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12"/></svg></div><span style="font-size:10px;color:#a0a0a0">Upload</span></div><input type="file" accept="image/*" style="display:none"/></label><div class="divider"></div><div><div class="section-label">Product Details</div><div class="field-group"><div><label class="field-label">Product / Service Name *</label><input type="text" placeholder="e.g. iPhone 15 Pro \u2014 256GB Black" class="field"/></div><div><label class="field-label">Product Link (optional)</label><input type="url" placeholder="https://olx.com/ad/\u2026" class="field"/></div><div class="field-row"><div><label class="field-label">Amount (PKR) *</label><input type="number" placeholder="0" class="field"/></div><div><label class="field-label">Delivery Charges</label><input type="number" placeholder="0" class="field"/></div></div></div></div></div></div><div class="card"><div class="card-inner"><div class="section-label">Fee Breakdown</div><div class="brow"><span style="color:#e0e0e0">Product</span><span>PKR \u2014</span></div><div class="brow"><span style="color:#e0e0e0">Delivery</span><span>PKR \u2014</span></div><div class="brow"><span style="color:#e0e0e0">Safely Fee (5%)</span><span>PKR \u2014</span></div><div class="brow brow-total"><span>Total</span><span>PKR \u2014</span></div></div></div><div class="card"><div class="card-inner card-gap"><div><div class="sp-card-title">Seller &amp; price</div><p class="sp-card-desc">Funds are held in escrow until you confirm delivery.</p></div><div style="display:flex;gap:8px"><button type="button" class="pm-pill pm-active" id="safely-pill-handle"><svg width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z"/></svg>Safely Handle</button><button type="button" class="pm-pill" id="safely-pill-whatsapp"><svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor"><path d="M17.472 14.382c-.297-.149-1.758-.867-2.03-.967-.273-.099-.471-.148-.67.15-.197.297-.767.966-.94 1.164-.173.199-.347.223-.644.075-.297-.15-1.255-.463-2.39-1.475-.883-.788-1.48-1.761-1.653-2.059-.173-.297-.018-.458.13-.606.134-.133.298-.347.446-.52.149-.174.198-.298.298-.497.099-.198.05-.371-.025-.52-.075-.149-.669-1.612-.916-2.207-.242-.579-.487-.5-.669-.51-.173-.008-.371-.01-.57-.01-.198 0-.52.074-.792.372-.272.297-1.04 1.016-1.04 2.479 0 1.462 1.065 2.875 1.213 3.074.149.198 2.096 3.2 5.077 4.487.709.306 1.262.489 1.694.625.712.227 1.36.195 1.871.118.571-.085 1.758-.719 2.006-1.413.248-.694.248-1.289.173-1.413-.074-.124-.272-.198-.57-.347zM12 2C6.477 2 2 6.477 2 12c0 1.89.525 3.66 1.438 5.168L2 22l4.832-1.438A9.955 9.955 0 0012 22c5.523 0 10-4.477 10-10S17.523 2 12 2z"/></svg>WhatsApp No.</button></div><div class="seller-block" id="safely-block-handle"><label class="field-label">Seller Handle *</label><div style="position:relative"><span style="position:absolute;left:14px;top:50%;transform:translateY(-50%);font-size:15px;font-weight:700;color:#8e8e93;pointer-events:none">@</span><input type="text" placeholder="aliphones" class="field" style="padding-left:30px;text-transform:lowercase"/></div><div class="seller-lookup-card"><svg width="16" height="16" fill="none" stroke="#1d9bf0" stroke-width="2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"/></svg><div><div class="seller-lookup-name">@aliphones \u00b7 Ali Phones</div><div class="seller-lookup-meta">Verified seller \u00b7 Lahore</div></div></div></div><div class="seller-block method-hidden" id="safely-block-whatsapp"><div><label class="field-label">Seller Name *</label><input type="text" placeholder="Full name or shop name" class="field"/></div><div><label class="field-label">Seller WhatsApp *</label><div class="phone-row"><div class="phone-prefix">\ud83c\uddf5\ud83c\uddf0 +92</div><input type="tel" placeholder="300 1234567" class="field" style="flex:1" inputmode="numeric"/></div><p class="phone-hint">Safely sends the seller a WhatsApp message with order details and accept link.</p></div></div></div></div><button type="button" class="btn btn-black" id="safely-to-step2">Continue to Payment \u2192</button></div>' +
    '<div class="sub" id="safely-step-2"><button type="button" class="back-link" id="safely-back-step1">\u2190 Back to Details</button><div class="card"><div class="product-preview"><svg width="36" height="36" fill="none" stroke="#636366" stroke-width="1.2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z"/></svg><span class="product-preview-badge">iPhone 15 Pro</span></div><div class="card-inner"><div class="section-label">Order Summary</div><div class="brow"><span style="color:#e0e0e0">Product</span><span>PKR 250,000</span></div><div class="brow"><span style="color:#e0e0e0">Delivery</span><span>PKR 500</span></div><div class="brow"><span style="color:#e0e0e0">Safely Fee (5%)</span><span>PKR 12,525</span></div><div class="brow brow-total"><span>Total to Pay</span><span>PKR 263,025</span></div></div></div><div class="banking-qr-container"><div class="banking-qr-title">Payment QR Code</div><div class="banking-qr-image"><svg width="90" height="90" fill="none" stroke="#000000" stroke-width="1.5" viewBox="0 0 24 24"><rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/><rect x="9" y="9" width="6" height="6" rx="0.5"/></svg></div><div class="banking-qr-amount">Scan to pay PKR 263,025</div><div class="banking-details"><p>Account Name: <span>Safely Escrow</span></p><p>Account Number: <span>03XX-XXXX789</span></p><p>Bank: <span>Easypaisa</span></p><p>Reference: <span>COV-A3BX-7KPQ</span></p></div></div><div class="banner banner-green"><div style="display:flex;align-items:flex-start;gap:10px"><span style="font-size:18px;line-height:1.4;flex-shrink:0">\ud83d\udd12</span><div><div style="margin-bottom:6px">Send <strong>PKR 263,025</strong> to Safely\'s escrow account</div><div style="margin-bottom:8px">via your saved method <strong>(Easypaisa)</strong></div><div style="font-size:12px;opacity:0.8">Then upload your transaction screenshot below.</div></div></div></div><div class="field-group"><div><label class="field-label">Transaction / Reference ID *</label><input type="text" placeholder="e.g. TXN-123456789" class="field"/></div><div><label class="field-label">Payment Screenshot</label><label class="upload-zone" style="height:90px"><svg width="22" height="22" fill="none" stroke="#636366" stroke-width="1.5" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z"/></svg><span style="font-size:13px;color:#636366">Attach payment screenshot</span><input type="file" accept="image*"/></label></div><div><label class="field-label">Delivery Address *</label><textarea placeholder="House 12, Street 4, G-9/2, Islamabad" class="field"></textarea></div></div><button type="button" class="btn btn-black" id="safely-to-step3">Confirm \u2014 I\'ve Paid</button></div>' +
    '<div class="sub" id="safely-step-3" style="align-items:center;padding-top:16px;gap:20px"><div class="success-icon"><svg width="26" height="26" fill="none" stroke="#000" stroke-width="2.5" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg></div><div><h3 class="success-heading">Payment Submitted</h3><p class="success-desc">Share your order ID with the seller to begin verification.</p></div><div class="card" style="width:100%"><div class="card-inner"><div class="section-label">What you paid</div><div class="brow"><span style="color:#e0e0e0">Product</span><span>PKR 250,000</span></div><div class="brow"><span style="color:#e0e0e0">Delivery</span><span>PKR 500</span></div><div class="brow"><span style="color:#e0e0e0">Safely Fee (5%)</span><span>PKR 12,525</span></div><div class="brow brow-total"><span>Total Paid</span><span>PKR 263,025</span></div></div></div><div class="order-id-box"><div class="order-id-label">Your Order ID</div><div class="order-id-value mono">COV-A3BX-7KPQ</div></div></div></div>' +
    '<div id="safely-sub-seller" style="display:none;">' +
    sellerTabHTML +
    "</div>";

  window.__safelyAddTab(
    "protect",
    "Protect",
    protectTabHTML,
    '<svg viewBox="0 0 24 24" fill="none" stroke="#8e8e93" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3L4 7v5c0 5.5 3.6 9.3 8 10.3C16.4 21.3 20 17.5 20 12V7L12 3z"/></svg>',
    function (root) {
      function switchProtectSub(sub) {
        protectSubTab = sub;
        var o = document.getElementById("safely-sub-orders");
        var n = document.getElementById("safely-sub-new-order");
        var s = document.getElementById("safely-sub-seller");
        if (o) o.style.display = sub === "orders" ? "block" : "none";
        if (n) n.style.display = sub === "new-order" ? "block" : "none";
        if (s) s.style.display = sub === "seller" ? "block" : "none";
        root
          .querySelectorAll("#safely-protect-subs .safely-sub-tab")
          .forEach(function (b) {
            b.classList.toggle("safely-active", b.dataset.sub === sub);
          });
        var area = root.querySelector(".safely-tabs-area");
        if (area) area.scrollTop = 0;
      }
      root
        .querySelectorAll("#safely-protect-subs .safely-sub-tab")
        .forEach(function (btn) {
          btn.addEventListener("click", function (e) {
            e.stopPropagation();
            switchProtectSub(btn.dataset.sub);
          });
        });

      function updateStepIndicator(step) {
        for (var i = 1; i <= 3; i++) {
          var dot = document.getElementById("safely-dot-" + i);
          var stepEl = document.getElementById("safely-step-" + i);
          var stepLabel = dot ? dot.parentElement : null;
          if (!dot || !stepEl || !stepLabel) continue;
          dot.classList.remove("dot-active", "dot-done");
          stepLabel.classList.remove("step-active");
          if (i < step) {
            dot.classList.add("dot-done");
            dot.innerHTML =
              '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>';
          } else if (i === step) {
            dot.classList.add("dot-active");
            dot.textContent = i;
            stepLabel.classList.add("step-active");
          } else {
            dot.textContent = i;
          }
          stepEl.classList.remove("sub-visible");
        }
        var active = document.getElementById("safely-step-" + step);
        if (active) active.classList.add("sub-visible");
      }
      function goStep(step) {
        formStep = step;
        updateStepIndicator(step);
        var area = root.querySelector(".safely-tabs-area");
        if (area) area.scrollTop = 0;
      }
      var toStep2 = document.getElementById("safely-to-step2");
      if (toStep2)
        toStep2.addEventListener("click", function (e) {
          e.stopPropagation();
          goStep(2);
        });
      var backStep1 = document.getElementById("safely-back-step1");
      if (backStep1)
        backStep1.addEventListener("click", function (e) {
          e.stopPropagation();
          goStep(1);
        });
      var toStep3 = document.getElementById("safely-to-step3");
      if (toStep3)
        toStep3.addEventListener("click", function (e) {
          e.stopPropagation();
          goStep(3);
        });
      root
        .querySelectorAll("#safely-step-row .new-step")
        .forEach(function (el) {
          el.addEventListener("click", function (e) {
            e.stopPropagation();
            var s = parseInt(el.dataset.step, 10);
            if (!isNaN(s) && s <= formStep) goStep(s);
          });
        });

      var pillHandle = document.getElementById("safely-pill-handle");
      var pillWhatsapp = document.getElementById("safely-pill-whatsapp");
      var blockHandle = document.getElementById("safely-block-handle");
      var blockWhatsapp = document.getElementById("safely-block-whatsapp");
      function switchSellerMethod(method) {
        sellerMethod = method;
        if (pillHandle)
          pillHandle.classList.toggle("pm-active", method === "handle");
        if (pillWhatsapp)
          pillWhatsapp.classList.toggle("pm-active", method === "whatsapp");
        if (blockHandle)
          blockHandle.classList.toggle("method-hidden", method !== "handle");
        if (blockWhatsapp)
          blockWhatsapp.classList.toggle(
            "method-hidden",
            method !== "whatsapp",
          );
      }
      if (pillHandle)
        pillHandle.addEventListener("click", function (e) {
          e.stopPropagation();
          switchSellerMethod("handle");
        });
      if (pillWhatsapp)
        pillWhatsapp.addEventListener("click", function (e) {
          e.stopPropagation();
          switchSellerMethod("whatsapp");
        });

      var selViews = [
        "safely-sel-lookup",
        "safely-sel-order",
        "safely-sel-ship",
        "safely-sel-shipped",
      ];
      function updateSellerStepIndicator(step) {
        for (var i = 1; i <= 3; i++) {
          var dot = document.getElementById("safely-sel-dot-" + i);
          var stepLabel = dot ? dot.parentElement : null;
          if (!dot || !stepLabel) continue;
          dot.classList.remove("safely-sel-dot-active", "safely-sel-dot-done");
          stepLabel.classList.remove("safely-sel-active");
          if (i < step) {
            dot.classList.add("safely-sel-dot-done");
            dot.innerHTML =
              '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>';
          } else if (i === step) {
            dot.classList.add("safely-sel-dot-active");
            dot.textContent = i;
            stepLabel.classList.add("safely-sel-active");
          } else {
            dot.textContent = i;
          }
        }
        selViews.forEach(function (id) {
          var el = document.getElementById(id);
          if (el) el.classList.remove("sub-visible");
        });
        var viewIndex = step <= 3 ? step - 1 : 3;
        var activeView = document.getElementById(selViews[viewIndex]);
        if (activeView) activeView.classList.add("sub-visible");
      }
      function goSellerStep(step) {
        sellerStep = step;
        updateSellerStepIndicator(step);
        var area = root.querySelector(".safely-tabs-area");
        if (area) area.scrollTop = 0;
      }
      var selLookupBtn = document.getElementById("safely-sel-lookup-btn");
      if (selLookupBtn)
        selLookupBtn.addEventListener("click", function (e) {
          e.stopPropagation();
          goSellerStep(2);
        });
      var selBackLookup = document.getElementById("safely-sel-back-lookup");
      if (selBackLookup)
        selBackLookup.addEventListener("click", function (e) {
          e.stopPropagation();
          goSellerStep(1);
        });
      var selReject = document.getElementById("safely-sel-reject");
      if (selReject)
        selReject.addEventListener("click", function (e) {
          e.stopPropagation();
          goSellerStep(1);
        });
      var selAccept = document.getElementById("safely-sel-accept");
      if (selAccept)
        selAccept.addEventListener("click", function (e) {
          e.stopPropagation();
          goSellerStep(3);
        });
      var selBackOrder = document.getElementById("safely-sel-back-order");
      if (selBackOrder)
        selBackOrder.addEventListener("click", function (e) {
          e.stopPropagation();
          goSellerStep(2);
        });
      var selConfirmShip = document.getElementById("safely-sel-confirm-ship");
      if (selConfirmShip)
        selConfirmShip.addEventListener("click", function (e) {
          e.stopPropagation();
          goSellerStep(4);
        });
      root
        .querySelectorAll("#safely-sel-step-row .safely-sel-step")
        .forEach(function (el) {
          el.addEventListener("click", function (e) {
            e.stopPropagation();
            var s = parseInt(el.dataset.sstep, 10);
            if (!isNaN(s) && s <= sellerStep) goSellerStep(s);
          });
        });

      if (window.__safelyPreventInputBubbling)
        window.__safelyPreventInputBubbling();
    },
  );
})();
