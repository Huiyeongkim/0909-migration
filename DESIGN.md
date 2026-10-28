---
name: 0909 Commerce
description: 서울의 공동구매 상품을 밀도 높은 흑백 편집 카탈로그로 보여주는 커머스 시스템
colors:
  ink: "#0a0a0a"
  paper: "#f4f4f1"
  white: "#ffffff"
  safety-orange: "#d93600"
  rule: "#a9a9a4"
  muted: "#666661"
  danger: "#a1190e"
typography:
  display:
    fontFamily: "Arial Black, Pretendard, sans-serif"
    fontSize: "clamp(54px, 7.2vw, 116px)"
    fontWeight: 900
    lineHeight: 0.87
    letterSpacing: "-0.09em"
  headline:
    fontFamily: "Arial Black, Pretendard, sans-serif"
    fontSize: "clamp(45px, 7vw, 100px)"
    fontWeight: 900
    lineHeight: 0.95
    letterSpacing: "-0.075em"
  title:
    fontFamily: "Pretendard, -apple-system, BlinkMacSystemFont, Segoe UI, sans-serif"
    fontSize: "clamp(19px, 1.8vw, 30px)"
    fontWeight: 800
    lineHeight: 1.13
    letterSpacing: "-0.055em"
  body:
    fontFamily: "Pretendard, -apple-system, BlinkMacSystemFont, Segoe UI, sans-serif"
    fontSize: "16px"
    fontWeight: 400
    lineHeight: 1.75
  label:
    fontFamily: "Arial, sans-serif"
    fontSize: "10px"
    fontWeight: 700
    lineHeight: 1
    letterSpacing: "0.1em"
rounded:
  sharp: "0"
  indicator: "50%"
spacing:
  xs: "8px"
  sm: "12px"
  md: "16px"
  lg: "24px"
  xl: "40px"
components:
  button-primary:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.white}"
    typography: "{typography.label}"
    rounded: "{rounded.sharp}"
    padding: "10px 16px"
    height: "42px"
  button-secondary:
    backgroundColor: "{colors.white}"
    textColor: "{colors.ink}"
    typography: "{typography.label}"
    rounded: "{rounded.sharp}"
    padding: "10px 16px"
    height: "42px"
  input:
    backgroundColor: "{colors.white}"
    textColor: "{colors.ink}"
    typography: "{typography.body}"
    rounded: "{rounded.sharp}"
    padding: "9px 11px"
    height: "43px"
  status-badge:
    backgroundColor: "{colors.safety-orange}"
    textColor: "{colors.white}"
    typography: "{typography.label}"
    rounded: "{rounded.sharp}"
    padding: "8px 10px"
---

# Design System: 0909 Commerce

## Overview

**Creative North Star: "Seoul Editorial Market"**

0909 Commerce interprets a Seoul fashion-editorial sensibility as a product-first group-buying catalog. Matte black, warm paper white, dense Korean typography, documentary product imagery, and sharp rules make price, status, participation, and timing read before decoration.

The system is deliberately flat and asymmetric. One featured product establishes hierarchy while compact neighboring entries preserve comparison density; responsive layouts keep that hierarchy instead of turning every item into the same card.

**Key Characteristics:**

- Product-first asymmetric catalog composition
- Matte black and warm white with rare safety-orange emphasis
- Dense, tightly tracked type and one-pixel structural rules
- Sharp rectangular controls and containers
- Fixed bottom navigation on mobile

## Colors

The palette is nearly monochrome; safety orange is reserved for transaction-critical emphasis and current state.

### Primary

- **Safety Orange:** Use for current or scheduled status, price emphasis, selection, and high-visibility focus feedback.

### Neutral

- **Matte Ink:** Primary text, navigation bars, progress fill, borders, and filled actions.
- **Warm Paper:** Page ground; its slight warmth keeps the catalog from feeling clinical.
- **Clean White:** Product, form, list, and card surfaces.
- **Editorial Rule:** Dividers and low-emphasis borders that organize dense information.
- **Muted Graphite:** Supporting copy, metadata, and secondary values.
- **Deep Error Red:** Destructive actions and error states only.

### Named Rules

**The One Accent Rule.** Safety orange signals price, status, selection, or focus; it is never ambient decoration.

**The Black-and-White First Rule.** A screen must remain legible and hierarchically complete before orange is applied.

## Typography

**Display Font:** Arial Black with Pretendard and sans-serif fallbacks  
**Body Font:** Pretendard with system sans-serif fallbacks  
**Label Font:** Arial with sans-serif fallback

**Character:** Oversized, tightly tracked display type supplies editorial force. Neutral Korean body text and compact uppercase metadata keep dense commerce facts readable.

### Hierarchy

- **Display:** Extra-black, compressed line-height; hero statements only.
- **Headline:** Extra-black; section and account-page headings.
- **Title:** Bold and tightly tracked; product names and card hierarchy.
- **Body:** Regular with open leading; descriptions and explanatory copy.
- **Label:** Bold, uppercase where appropriate, with wide tracking; categories, catalog numbers, eyebrows, and compact metadata.

### Named Rules

**The Compression Rule.** Tight tracking and compressed leading belong to large headings; body copy retains open leading and normal spacing.

## Layout

Desktop content is centered within a wide 1480px maximum canvas with fluid side gutters. The catalog uses a two-column grid with the first item spanning two rows; within cards, image and commerce data may divide the cell asymmetrically. Detail pages use three columns for image, facts, and action.

At 1080px, cards stack image over copy and detail actions span the grid. At 760px, content becomes edge-to-edge, detail content becomes one column, the featured catalog item spans both columns, and five navigation destinations move to a fixed 67px bottom bar. At 430px, labels and image heights tighten while the two-column comparison grid remains.

Spacing is compact and rhythmic: 8–16px inside controls and metadata, 20–40px inside content panels, with large fluid padding reserved for hero and page headings.

**The Product Hierarchy Rule.** Preserve one dominant catalog item and dense comparison neighbors; do not normalize the catalog into equal promotional tiles.

## Elevation & Depth

The system is flat by default and uses no ambient card shadows. Depth comes from black or gray one-pixel rules, alternating neutral surfaces, image/copy divisions, and sticky navigation. Offset hard shadows are limited to transient feedback messages.

### Named Rules

**The Structural Depth Rule.** Use rules and tonal separation for persistent hierarchy; reserve hard shadow for temporary feedback.

## Shapes

Cards, inputs, buttons, panels, and badges use sharp rectangular corners. One-pixel borders remain visible and purposeful. Circular geometry is limited to tiny active-navigation indicators; it is not a container language.

**The Sharp Edge Rule.** Do not round application cards or controls; the catalog's precision depends on square silhouettes.

## Components

### Buttons

- **Shape:** Sharp rectangle with a one-pixel ink border.
- **Primary:** Matte ink fill, white bold label, compact horizontal padding.
- **Hover / Focus:** Hover inverts black and white. Keyboard focus uses a clearly offset translucent orange outline; disabled buttons retain form and reduce opacity.
- **Secondary:** White fill with ink text; hover inverts to ink.
- **Danger:** White fill with deep red text; hover fills deep red.

### Chips

- **Style:** Compact square status badges use ink by default, safety orange for scheduled status, and deep red for failure.
- **State:** Text remains explicit; color never carries status alone.

### Cards / Containers

- **Corner Style:** Square.
- **Background:** White content surfaces on warm paper.
- **Shadow Strategy:** None at rest.
- **Border:** One-pixel editorial rules divide image, status, copy, and actions.
- **Internal Padding:** Compact on catalog cards; fluid and larger on detail and account panels.

### Inputs / Fields

- **Style:** White surface, one-pixel gray rule, sharp corners, and a 43px minimum height.
- **Focus:** Offset translucent orange outline.
- **Error / Disabled:** Error meaning is stated in text; disabled actions reduce opacity and change cursor.

### Navigation

Desktop navigation lives in the sticky matte-black top bar with divided tabs, white active text, a darker active cell, and a small orange indicator. At mobile width it becomes a fixed white bottom bar with five equal destinations, numeric indices, vertical dividers, and the same orange active indicator.

### Product Catalog Card

The signature card joins documentary imagery with category, explicit status, product title, safety-orange price, struck original price, participation progress, quantity, timing, and action. The first card is visually dominant; neighboring cards remain compact and comparison-oriented.

## Do's and Don'ts

### Do:

- **Do** keep product imagery and purchase facts ahead of promotional copy.
- **Do** use one-pixel rules and neutral surface changes to structure dense layouts.
- **Do** preserve visible focus, text status labels, and reduced-motion behavior.
- **Do** keep the mobile five-destination navigation fixed to the bottom edge.

### Don't:

- **Don't** use gradients, glass effects, or soft ambient shadows.
- **Don't** round cards, fields, buttons, badges, or navigation cells.
- **Don't** use safety orange as a decorative background or broad brand wash.
- **Don't** replace the asymmetric catalog with a uniform app-card grid.
