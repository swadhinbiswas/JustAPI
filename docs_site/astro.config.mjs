import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  integrations: [
    starlight({
      title: 'JustAPI',
      description: 'Rust-powered, FastAPI-class web framework for Python. 700k+ RPS, zero-GIL execution.',
      logo: {
        src: './src/assets/logo.svg',
        replacesTitle: false,
      },
      social: {
        github: 'https://github.com/swadhinbiswas/JustAPI',
      },
      customCss: ['./src/styles/custom.css'],
      sidebar: [
        {
          label: 'Getting Started',
          autogenerate: { directory: 'getting-started' },
        },
        {
          label: 'Deployment',
          autogenerate: { directory: 'deployment' },
        },
      ],

    }),
  ],
});
