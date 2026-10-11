import { render, screen } from '@testing-library/react';
import { App } from './App.tsx';

test('renders the product name', () => {
  render(<App />);
  expect(screen.getByRole('heading', { name: 'FileDock' })).toBeInTheDocument();
});
