---
title: Verificador de Domínios Parecidos
description: Identifique sites e e-mails falsos que copiam o nome de uma empresa real trocando uma letra. Grátis e sem cadastro.
date: 2026-11-10
image: /images/tool-banners/lookalike-domain-checker.svg
---

## O que esta ferramenta verifica

Golpistas copiam o nome do site de uma empresa em que você confia e mudam só o suficiente para você não perceber. Um e-mail de `vendas@alibabba.com` ou uma página de pagamento em `paypa1-seguro.com` parece certo numa olhada rápida.

Digite o site ou o e-mail que você recebeu. Se você sabe qual é o site real da empresa, coloque no segundo campo, e a ferramenta compara os dois. Se deixar vazio, a ferramenta compara o nome com marcas famosas como Amazon, PayPal, Alibaba, DHL e os grandes marketplaces B2B.

## Os truques que ela encontra

| Truque | Falso | Real |
|---|---|---|
| Uma letra trocada, a mais, a menos ou invertida | `alibabba.com`, `amazn.com`, `goolge.com` | `alibaba.com`, `amazon.com`, `google.com` |
| Um número no lugar de uma letra | `paypa1.com`, `amaz0n.com` | `paypal.com`, `amazon.com` |
| Letras que juntas parecem outra | `rnicrosoft.com` (rn parece m) | `microsoft.com` |
| Letras de outro alfabeto | `аpple.com` (um "а" cirílico) | `apple.com` |
| Uma palavra ou hífen a mais | `paypal-seguro.com`, `pay-pal.com` | `paypal.com` |
| Outra terminação | `paypal.co` | `paypal.com` |
| O nome real na frente de outro site | `paypal.com.login-ajuda.xyz` | `paypal.com` |

O último engana muita gente: o endereço começa com o nome real, mas o site é na verdade `login-ajuda.xyz`. O que vale é a parte logo antes da terminação, não o começo.

## Como ler o resultado

| Resultado | O que significa |
|---|---|
| Imitação | Uma cópia com um truque. Não entre na conta, não pague e não baixe nada dele. |
| Parecido | Parecido com o nome real, mas não é uma cópia clara. Pode ser o site da empresa para outro país. Pergunte à empresa. |
| Não é uma cópia | Um nome diferente. Também não é o site da empresa real. |
| Site real | O mesmo site do real, ou uma parte dele, como `loja.paypal.com`. |

> **Dica:** Uma conta bancária "nova" enviada por um e-mail de endereço parecido é um dos golpes de pagamento B2B mais comuns. Antes de pagar para novos dados bancários, ligue para o fornecedor em um número que você mesmo encontrou, não o do e-mail.

## Perguntas

### Esta ferramenta é grátis?

Sim. Não precisa de cadastro e não há limite. A verificação roda no nosso servidor na hora e não consulta nada na internet.

### "Não é uma cópia" quer dizer que o site é seguro?

Não. Só quer dizer que o nome não copia a empresa que você informou. Veja a idade do site com o [Verificador de Idade de Domínio](/pt-br/tools/domain-age-checker) e procure a empresa no registro oficial.

### Vocês guardam os endereços que eu verifico?

Não. O endereço é verificado e a resposta aparece para você. Nada é guardado.

### Posso verificar um fornecedor automaticamente?

Sim. A extensão grátis do Safely para o Chrome verifica nomes parecidos, e outros 14 sinais, sempre que você abre a página de um fornecedor no Alibaba e em outros marketplaces B2B.
