import React from 'react';
import { Layers, Terminal, Activity, Package, Archive, Sliders, Globe, Database, Shield } from 'lucide-react';
import { ViewTab } from '../types';

interface NavRailProps {
  activeTab: ViewTab;
  onTabChange: (tab: ViewTab) => void;
}

export const NavRail: React.FC<NavRailProps> = ({ activeTab, onTabChange }) => {
  const navItems: { id: ViewTab; label: string; icon: React.ReactNode }[] = [
    { id: 'fleet', label: 'Fleet Overview', icon: <Layers size={18} /> },
    { id: 'console', label: 'Live Console', icon: <Terminal size={18} /> },
    { id: 'diagnostics', label: 'AI Diagnostics', icon: <Activity size={18} /> },
    { id: 'plugins', label: 'Plugins & Mods', icon: <Package size={18} /> },
    { id: 'backups', label: 'Backup & DR Hub', icon: <Archive size={18} /> },
    { id: 'config', label: 'Configuration Studio', icon: <Sliders size={18} /> },
    { id: 'edge', label: 'Edge Mesh', icon: <Globe size={18} /> },
    { id: 'storage', label: 'Storage & DR', icon: <Database size={18} /> },
    { id: 'audit', label: 'Audit Trail', icon: <Shield size={18} /> },
  ];

  return (
    <nav className="nav-rail">
      {navItems.map((item) => (
        <button
          key={item.id}
          className={`nav-item ${activeTab === item.id ? 'active' : ''}`}
          onClick={() => onTabChange(item.id)}
          title={item.label}
        >
          {item.icon}
        </button>
      ))}
    </nav>
  );
};
