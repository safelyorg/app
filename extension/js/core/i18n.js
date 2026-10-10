"use strict";
// Safely extension language: English or Portuguese (Brazil).
//
// How the language is chosen:
// 1. The person's own choice (the EN / PT button in the panel header),
//    saved in chrome.storage.local as "safely_lang".
// 2. Otherwise the Chrome language: Portuguese Chrome -> Portuguese.
// 3. Otherwise English.
//
// The English text itself is the key: t("Sign in free") returns
// "Entrar grátis" in Portuguese and "Sign in free" in English. A text
// that is not in the list below simply stays in English, so nothing
// ever shows blank.
//
// Every other script reads this through window.__safelyI18n and falls
// back to plain English when it is missing (the tests load files one
// at a time, without this one).
//
// The result text written by Safely (the reasons under each check and
// the risk factor explanations) is translated by the backend: the
// extension sends "language" with every scan, and when the language is
// changed on a result already showing, panel.ts asks the backend for
// that scan's saved translation (the same one the dashboard shows).
//
// These words are the same as the dashboard's
// (dashboard/locales/pt-br.json) - change both together.
//
// To add a language: add its code to LANGS, add its word list next to
// PT, and add it to backend/services/translation.rs.
(function () {
    "use strict";
    const STORAGE_KEY = "safely_lang";
    const LANGS = ["en", "pt-br"];
    const PT = {
        // ---------- panel ----------
        "Safely doesn't check this page — open a listing on a supported marketplace to scan it.": "O Safely não verifica esta página — abra um anúncio em um marketplace compatível para analisá-lo.",
        "Sign in free to analyze this listing. You get 10 free scans every month — no credit card needed.": "Entre grátis para analisar este anúncio. Você tem 10 verificações grátis por mês — sem cartão de crédito.",
        "Sign in free": "Entrar grátis",
        Reload: "Recarregar",
        "You've used all your scans for this month.": "Você usou todas as suas verificações deste mês.",
        "See plans": "Ver planos",
        "Not a listing page": "Não é uma página de anúncio",
        "Sign in required": "É preciso entrar",
        "Scan limit reached": "Limite de verificações atingido",
        "Couldn't analyze this listing": "Não foi possível analisar este anúncio",
        "You've checked several listings quickly. Try again in {time}.": "Você verificou vários anúncios em pouco tempo. Tente novamente em {time}.",
        "You can check another listing now.": "Você já pode verificar outro anúncio.",
        "Couldn't analyze this listing right now. Please try again in a moment.": "Não foi possível analisar este anúncio agora. Tente novamente em instantes.",
        "Change language": "Mudar idioma",
        Risk: "Risco",
        Intelligence: "Inteligência",
        Report: "Denúncia",
        // ---------- plan / scan limit ----------
        "your next monthly reset": "a próxima renovação mensal",
        Unlimited: "Ilimitado",
        "You've used your {limit} free scans for this month. Upgrade to Team or Enterprise to keep scanning now, or your free scans come back on {date}.": "Você usou suas {limit} verificações grátis deste mês. Assine o Team ou o Enterprise para continuar agora, ou suas verificações grátis voltam em {date}.",
        "You've used all {limit} scans included in your plan this month.": "Você usou todas as {limit} verificações incluídas no seu plano este mês.",
        "You've used all the scans included in your plan this month.": "Você usou todas as verificações incluídas no seu plano este mês.",
        // ---------- risk tab ----------
        "Low risk": "Baixo risco",
        Caution: "Risco moderado",
        "High risk": "Alto risco",
        "No major warnings found": "Nenhum alerta importante encontrado",
        "Not enough information": "Informações insuficientes",
        "Check this supplier carefully yourself": "Verifique este fornecedor com cuidado",
        "Before you pay, check:": "Antes de pagar, confira:",
        "Pay through the platform's buyer protection, or only to a bank account in the company's own name": "Pague pela proteção ao comprador da plataforma, ou só para uma conta bancária no nome da própria empresa",
        "Confirm the bank details on a video call": "Confirme os dados bancários em uma chamada de vídeo",
        "Don't pay the full amount before delivery": "Não pague o valor total antes da entrega",
        "Review before proceeding": "Revise antes de prosseguir",
        "High risk detected": "Alto risco detectado",
        "Seller Information": "Informações do Vendedor",
        Username: "Usuário",
        Phone: "Telefone",
        "Account age": "Idade da conta",
        Location: "Localização",
        "Last active": "Última atividade",
        Status: "Status",
        "Fraud Reports": "Denúncias de fraude",
        Platform: "Plataforma",
        "Visit activity — 12 months": "Atividade de visitas · 12 meses",
        Jan: "Jan",
        Feb: "Fev",
        Mar: "Mar",
        Apr: "Abr",
        May: "Mai",
        Jun: "Jun",
        Jul: "Jul",
        Aug: "Ago",
        Sep: "Set",
        Oct: "Out",
        Nov: "Nov",
        Dec: "Dez",
        "I'm proceeding": "Vou prosseguir",
        "I'm backing out": "Vou desistir",
        "Thanks - your response has been recorded.": "Obrigado — sua resposta foi registrada.",
        "No fraud reports found on the Safely network.": "Registro limpo na rede Safely. Nenhuma denúncia de fraude encontrada.",
        "1 fraud report found on the Safely network. Proceed with caution.": "1 denúncia de fraude encontrada na rede Safely. Prossiga com cautela.",
        "{n} fraud reports found on the Safely network. High-risk seller.": "{n} denúncias de fraude encontradas na rede Safely. Vendedor de alto risco.",
        "Report this seller": "Denunciar este vendedor",
        "If you experienced fraud or suspicious behavior from this seller, help protect others by submitting a report.": "Se você sofreu fraude ou viu um comportamento suspeito deste vendedor, ajude a proteger outras pessoas enviando uma denúncia.",
        "Select reason": "Escolha o motivo",
        Scam: "Golpe",
        "Seller took payment and disappeared": "O vendedor recebeu o pagamento e sumiu",
        "Fake item": "Produto falso",
        "Item was counterfeit or misrepresented": "O produto era falsificado ou diferente do anunciado",
        "No delivery": "Não entregue",
        "Payment sent but item never arrived": "Pagamento enviado, mas o produto nunca chegou",
        "Wrong item": "Produto errado",
        "Received something different": "Recebi algo diferente",
        "Non responsive": "Sem resposta",
        "Seller stopped responding after payment": "O vendedor parou de responder após o pagamento",
        "Submit Report": "Enviar denúncia",
        "Submitting...": "Enviando...",
        "Report submitted. Thank you for helping protect the community.": "Denúncia enviada. Obrigado por ajudar a proteger a comunidade.",
        "Please select a reason before submitting.": "Escolha um motivo antes de enviar.",
        "Please sign in again to submit a report.": "Entre novamente para enviar uma denúncia.",
        "Failed to submit report. Please try again.": "Não foi possível enviar a denúncia. Tente novamente.",
        unknown: "Desconhecido",
        reported: "Denunciado",
        verified: "Verificado",
        // ---------- intelligence tab ----------
        "All {n} signals checked. No red flags detected.": "Todos os {n} sinais verificados. Nenhum sinal de alerta detectado.",
        "{bad} of {n} signals need your attention.": "{bad} de {n} sinais precisam da sua atenção.",
        "Listing signals": "Sinais do anúncio",
        "Social presence check": "Verificação de presença social",
        "Click to drop down": "Clique para abrir",
        "Click to drop up": "Clique para fechar",
        "Click to see checks": "Clique para ver as verificações",
        "Click to hide checks": "Clique para ocultar as verificações",
        "Price vs market": "Preço vs mercado",
        "No price data available.": "Nenhum dado de preço disponível.",
        "Recommended checks": "Verificações recomendadas",
        "Risk Factors": "Fatores de risco",
        Serious: "Grave",
        "Pattern match": "Padrão identificado",
        "Worth noting": "Vale observar",
        Reviews: "Avaliações",
        "Contact (Facebook)": "Contato (Facebook)",
        "Contact (LinkedIn)": "Contato (LinkedIn)",
        "Contact (Web)": "Contato (Web)",
        // recommended checks: consumer marketplaces
        "Ask for a live video call": "Peça uma chamada de vídeo ao vivo",
        "Verify the item is physically in the seller's hands before sending any payment.": "Confirme que o produto está mesmo nas mãos do vendedor antes de enviar qualquer pagamento.",
        "Check IMEI on delivery": "Confira o IMEI na entrega",
        "Dial *#06# on the device and confirm the number matches what the seller declared at deal creation.": "Disque *#06# no aparelho e confirme que o número é o mesmo que o vendedor informou ao fechar o negócio.",
        "Do not pay to number in listing": "Não pague para o número do anúncio",
        "A phone number in the listing could route your payment outside Safely escrow protection.": "Um número de telefone no anúncio pode desviar seu pagamento para fora da proteção de garantia do Safely.",
        // recommended checks: B2B suppliers
        "Use buyer protection if the platform offers it": "Use a proteção ao comprador, se a plataforma oferecer",
        "If the platform has protected payment (e.g. Trade Assurance on Alibaba), pay through it so the order is covered. If it has none, pay only by bank transfer to an account in the company's own name.": "Se a plataforma tem pagamento protegido (por exemplo, o Trade Assurance do Alibaba), pague por ele para o pedido ficar coberto. Se não tiver, pague só por transferência bancária para uma conta no nome da própria empresa.",
        "Match the business licence to the bank account": "Compare a licença comercial com a conta bancária",
        "Ask for the business licence and check that the company name on it matches the listing and the name on the bank account exactly.": "Peça a licença comercial e confira se o nome da empresa nela é exatamente igual ao do anúncio e ao da conta bancária.",
        "Confirm bank details by phone or video": "Confirme os dados bancários por telefone ou vídeo",
        "Before the first payment, and whenever bank details change, confirm them live with a contact you already know. Never act on changed details sent only by email.": "Antes do primeiro pagamento, e sempre que os dados bancários mudarem, confirme-os ao vivo com um contato que você já conhece. Nunca aceite dados alterados enviados só por e-mail.",
        "Order a sample first": "Peça uma amostra primeiro",
        "Pay for a sample and check its quality before placing a bulk order.": "Pague por uma amostra e confira a qualidade antes de fazer um pedido grande.",
        "Ask the supplier to show where they work and your goods live: the production line if they are a factory, the warehouse or office if they are a trader or shipping company. This confirms they really operate where they say.": "Peça ao fornecedor para mostrar onde trabalha e onde estão suas mercadorias: a linha de produção, se for uma fábrica; o armazém ou o escritório, se for um revendedor ou uma empresa de transporte. Assim você confirma que ele realmente opera onde diz.",
        "Do not pay by Western Union, MoneyGram or crypto": "Não pague por Western Union, MoneyGram ou cripto",
        "This supplier lists payment methods that cannot be reversed or traced to a company. Pay only by bank transfer to an account in the company's own name, or through the platform's protected payment.": "Este fornecedor aceita formas de pagamento que não podem ser estornadas nem rastreadas até uma empresa. Pague só por transferência bancária para uma conta no nome da própria empresa, ou pelo pagamento protegido da plataforma.",
        "Don't pay everything before shipment": "Não pague tudo antes do envio",
        "This supplier asks for the full price before the goods are shipped. Try to pay the balance only against a copy of the Bill of Lading, or use a Letter of Credit, and pay only to a bank account in the company's own name.": "Este fornecedor pede o valor total antes de enviar a mercadoria. Tente pagar o saldo só contra a cópia do conhecimento de embarque (Bill of Lading), ou use uma carta de crédito, e pague só para uma conta bancária no nome da própria empresa.",
        "Buy only from the brand owner or an authorised distributor": "Compre só do dono da marca ou de um distribuidor autorizado",
        "This product needs a licence or prescription, and fakes of it can be dangerous. Ask the supplier for a letter from the brand owner showing they are an authorised distributor, and check it with the brand owner directly. You may also need your own import licence.": "Este produto exige licença ou receita, e versões falsas podem ser perigosas. Peça ao fornecedor uma carta do dono da marca mostrando que ele é distribuidor autorizado e confirme diretamente com o dono da marca. Você também pode precisar da sua própria licença de importação.",
        // ---------- check names (signal labels) ----------
        "Domain check": "Verificação de domínio",
        "Price analysis": "Análise de preço",
        "Urgency language": "Linguagem de urgência",
        "Advance payment request": "Solicitação de pagamento antecipado",
        "Duplicate listing": "Anúncio duplicado",
        "Listing detail": "Detalhes do anúncio",
        "Regulated product": "Produto regulamentado",
        "Image authenticity": "Autenticidade da imagem",
        "Overall legitimacy check": "Verificação geral de legitimidade",
        "Safely history": "Histórico no Safely",
        "Seller website check": "Verificação do site do vendedor",
        "Platform verification": "Verificação da plataforma",
        "Contact info": "Informações de contato",
        "Store page check": "Verificação da página da loja",
        "Seller track record": "Histórico do vendedor",
        "Registration consistency": "Consistência do registro",
        "Company profile completeness": "Completude do perfil da empresa",
        "Listing completeness": "Completude do anúncio",
        "Company details": "Dados da empresa",
        // ---------- check results (signal values) ----------
        Detected: "Detectado",
        "None found": "Nenhum encontrado",
        Confirmed: "Confirmado",
        "Not confirmed": "Não confirmado",
        Verified: "Verificado",
        Unverified: "Não verificado",
        "Not verified": "Não verificado",
        Suspicious: "Suspeito",
        Unregistered: "Não registrado",
        Registered: "Registrado",
        "Fully confirmed": "Totalmente confirmado",
        "Name confirmed only": "Só o nome confirmado",
        Unconfirmed: "Não confirmado",
        "Not offered": "Não oferecida",
        "No badge": "Sem selo",
        "Not provided": "Não informado",
        "Invalid date": "Data inválida",
        "Founded this year": "Fundada este ano",
        "Not shown on this platform": "Não exibido nesta plataforma",
        Specific: "Específico",
        Vague: "Vago",
        "Licence needed": "Exige licença",
        "Licence needed, not the maker": "Exige licença, não é o fabricante",
        "Untraceable payment": "Pagamento sem rastreio",
        "Full prepayment": "Pagamento total antecipado",
        "Not checked": "Não verificado",
        "Website found": "Site encontrado",
        "No website found": "Nenhum site encontrado",
        "Possible website found": "Possível site encontrado",
        "No store page found": "Nenhuma página de loja encontrada",
        "Couldn't be loaded": "Não carregou",
        "Candidates found": "Candidatos encontrados",
        "Scam mentions found": "Menções a golpe encontradas",
        "No presence found": "Nenhuma presença encontrada",
        "Checked by Alibaba": "Checado pelo Alibaba",
        "Paid membership": "Assinatura paga",
        "No recent orders": "Nenhum pedido recente",
        "Risky option listed": "Opção arriscada na lista",
        "Product range": "Linha de produtos",
        Certificates: "Certificados",
        "Commodity scam pattern": "Padrão de golpe com commodities",
        "Often used in scams": "Comum em golpes",
        "Look copied": "Parecem copiados",
        "Out of date": "Desatualizados",
        "Not listed": "Não informado",
        "Don't pay fees before an inspection": "Não pague taxas antes de uma inspeção",
        "Fake commodity sellers ask for fees, a deposit or document costs before anything ships. Pay nothing until the goods are inspected by a company you choose (such as SGS), and pay only by Letter of Credit.": "Vendedores falsos de commodities pedem taxas, depósito ou custos de documentos antes de qualquer envio. Não pague nada até que a mercadoria seja inspecionada por uma empresa escolhida por você (como a SGS) e pague somente por Carta de Crédito.",
        "Signal mix": "Resumo dos sinais",
        "Mostly: {type}": "Maioria: {type}",
        Good: "Bom",
        Info: "Informativo",
        Warning: "Atenção",
        "Red flag": "Alerta grave",
        "{n} of {total} cards": "{n} de {total} cartões",
        "Hover the ring to see each part.": "Passe o mouse no anel para ver cada parte.",
        "Info cards are not warnings, but each adds 5 points to the score (15 at most).": "Cartões informativos não são alertas, mas cada um soma 5 pontos ao risco (no máximo 15).",
        "New to Safely": "Novo no Safely",
        "Checked once before": "Verificado 1 vez antes",
        Normal: "Normal",
        Unverifiable: "Não verificável",
        Original: "Original",
        Unknown: "Desconhecido",
        "Not found": "Não encontrado",
        "This month": "Este mês",
        // ---------- checklist items ----------
        "Employee count": "Número de funcionários",
        "Sales revenue": "Receita de vendas",
        "Export percentage": "Percentual de exportação",
        "Unit price": "Preço unitário",
        "FOB price": "Preço FOB",
        "Minimum order quantity": "Quantidade mínima de pedido",
        "Payment type": "Forma de pagamento",
        "Preferred port": "Porto preferencial",
        "Production capacity": "Capacidade de produção",
        "Delivery timeframe": "Prazo de entrega",
        Incoterms: "Incoterms",
        "Packaging details": "Detalhes da embalagem",
    };
    const PT_MONTHS = [
        "janeiro", "fevereiro", "março", "abril", "maio", "junho",
        "julho", "agosto", "setembro", "outubro", "novembro", "dezembro",
    ];
    // Results that carry a number ("3/9 fields provided", "About 11 years").
    const plural = (n, one, many) => (n === "1" ? one : many);
    const PT_PATTERNS = [
        [/^(\d+)\/(\d+) fields provided$/, (m) => m[1] + "/" + m[2] + " campos preenchidos"],
        [/^About (\d+) years?$/, (m) => "Cerca de " + m[1] + plural(m[1], " ano", " anos")],
        [
            /^(\d+) years? (?:and )?(\d+) months?$/,
            (m) => m[1] + plural(m[1], " ano", " anos") + " e " + m[2] + plural(m[2], " mês", " meses"),
        ],
        [/^(\d+) years?$/, (m) => m[1] + plural(m[1], " ano", " anos")],
        [/^(\d+) months?$/, (m) => m[1] + plural(m[1], " mês", " meses")],
        [/^Checked (\d+) times before$/, (m) => "Verificado " + m[1] + " vezes antes"],
        [/^(\d+) prior checks?$/, (m) => m[1] + plural(m[1], " verificação anterior", " verificações anteriores")],
        [/^([\d.]+) rating, (\d+) listings$/, (m) => "Nota " + m[1] + ", " + m[2] + " anúncios"],
        [/^(\d+) orders?, ([\d.]+) rating$/, (m) => m[1] + plural(m[1], " pedido", " pedidos") + ", nota " + m[2]],
        [/^(\d+) orders?$/, (m) => m[1] + plural(m[1], " pedido", " pedidos")],
        [/^(.+) member$/, (m) => "Membro " + m[1]],
    ];
    function isLang(value) {
        return typeof value === "string" && LANGS.indexOf(value) !== -1;
    }
    function chromeLang() {
        const code = (navigator.language || "").toLowerCase();
        return code.indexOf("pt") === 0 ? "pt-br" : "en";
    }
    let lang = chromeLang();
    function fill(text, vars) {
        if (!vars)
            return text;
        return text.replace(/\{(\w+)\}/g, (whole, key) => Object.prototype.hasOwnProperty.call(vars, key) ? String(vars[key]) : whole);
    }
    function translate(en) {
        if (lang !== "pt-br" || !en)
            return en;
        if (Object.prototype.hasOwnProperty.call(PT, en))
            return PT[en];
        for (const [pattern, build] of PT_PATTERNS) {
            const match = en.match(pattern);
            if (match)
                return build(match);
        }
        return en;
    }
    function t(en, vars) {
        return fill(translate(en), vars);
    }
    // "November 14" style date for the scan-limit message.
    function monthDay(month, day) {
        if (lang !== "pt-br")
            return null;
        return day + " de " + PT_MONTHS[month - 1];
    }
    function announce() {
        window.dispatchEvent(new CustomEvent("safely-lang-changed", { detail: { lang } }));
    }
    function setLang(next) {
        if (!isLang(next) || next === lang)
            return;
        lang = next;
        try {
            chrome.storage.local.set({ [STORAGE_KEY]: next });
        }
        catch (e) {
            // Saving can fail if the extension was just reloaded; the panel
            // still switches for this page.
        }
        announce();
    }
    // Resolves once the saved choice has been read. A scan waits for
    // this, so it always asks for the right language.
    const ready = new Promise((resolve) => {
        try {
            chrome.storage.local.get(STORAGE_KEY, (result) => {
                const saved = result ? result[STORAGE_KEY] : undefined;
                if (isLang(saved) && saved !== lang) {
                    lang = saved;
                    announce();
                }
                resolve(lang);
            });
        }
        catch (e) {
            resolve(lang);
        }
    });
    // A choice made in another tab applies here too.
    try {
        chrome.storage.onChanged.addListener((changes, area) => {
            if (area !== "local" || !changes[STORAGE_KEY])
                return;
            const next = changes[STORAGE_KEY].newValue;
            if (isLang(next) && next !== lang) {
                lang = next;
                announce();
            }
        });
    }
    catch (e) {
        // No storage events available: the language just stays as it is.
    }
    window.__safelyI18n = {
        t,
        monthDay,
        setLang,
        ready,
        getLang: () => lang,
    };
})();
