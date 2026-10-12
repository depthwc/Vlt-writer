import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'

import en from './en'
import ru from './ru'
import uz from './uz'

export const LANGS = [
  { code: 'uz', label: 'Oʻzbekcha' },
  { code: 'ru', label: 'Русский' },
  { code: 'en', label: 'English' },
] as const

export type Lang = (typeof LANGS)[number]['code']

export function systemLang(): Lang {
  const l = (navigator.language || 'en').slice(0, 2)
  return l === 'uz' || l === 'ru' ? l : 'en'
}

void i18n.use(initReactI18next).init({
  resources: { en: { translation: en }, ru: { translation: ru }, uz: { translation: uz } },
  lng: systemLang(),
  fallbackLng: 'en',
  interpolation: { escapeValue: false },
})

export default i18n
