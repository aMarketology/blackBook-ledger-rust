// ============================================
// QUICK START: Add Blockchain Feed to Your Frontend
// ============================================

// 1️⃣ Copy this hook to your project
// File: hooks/useBlockchainFeed.ts

import { useState, useEffect } from 'react';

interface BlockchainActivity {
  timestamp: string;
  emoji: string;
  action_type: string;
  details: string;
}

export const useBlockchainFeed = () => {
  const [activities, setActivities] = useState<BlockchainActivity[]>([]);
  const [stats, setStats] = useState<any>(null);

  useEffect(() => {
    const fetchFeed = async () => {
      try {
        const res = await fetch('http://localhost:8080/ledger/json');
        const data = await res.json();
        setActivities(data.activities);
        setStats(data.stats);
      } catch (err) {
        console.error('Failed to fetch blockchain feed:', err);
      }
    };

    fetchFeed();
    const interval = setInterval(fetchFeed, 3000); // Update every 3 seconds
    return () => clearInterval(interval);
  }, []);

  return { activities, stats };
};

// ============================================

// 2️⃣ Use it in your component
// File: components/BlockchainFeed.tsx

import { useBlockchainFeed } from '@/hooks/useBlockchainFeed';

export default function BlockchainFeed() {
  const { activities, stats } = useBlockchainFeed();

  return (
    <div style={{
      background: 'linear-gradient(135deg, #0f0f23 0%, #1a1a3e 100%)',
      border: '2px solid #00ff41',
      borderRadius: '10px',
      padding: '20px',
      color: '#00ff41',
      fontFamily: 'monospace',
      maxHeight: '600px',
      overflowY: 'auto'
    }}>
      <h2 style={{ textAlign: 'center', marginBottom: '20px' }}>
        🔗 Live Blockchain Activity
      </h2>

      {/* Stats Bar */}
      <div style={{ 
        display: 'flex', 
        justifyContent: 'space-around', 
        marginBottom: '20px',
        fontSize: '14px',
        color: '#00ccff'
      }}>
        <span>📊 Activities: {stats?.total_activities || 0}</span>
        <span>📈 Markets: {stats?.active_markets || 0}</span>
        <span>🎲 Live Bets: {stats?.live_bets_active || 0}</span>
      </div>

      {/* Activity Feed */}
      <div>
        {activities.length === 0 ? (
          <p style={{ textAlign: 'center', color: '#888' }}>
            No activity yet...
          </p>
        ) : (
          activities.map((activity, index) => (
            <div key={index} style={{
              padding: '12px',
              marginBottom: '8px',
              background: 'rgba(0, 40, 20, 0.5)',
              borderLeft: '4px solid #00ff41',
              borderRadius: '5px',
              fontSize: '13px'
            }}>
              <span style={{ color: '#888', marginRight: '10px' }}>
                [{activity.timestamp}]
              </span>
              <span style={{ fontSize: '18px', marginRight: '8px' }}>
                {activity.emoji}
              </span>
              <strong style={{ color: '#00ccff', marginRight: '8px' }}>
                {activity.action_type}
              </strong>
              <span style={{ color: '#aaffaa' }}>
                {activity.details}
              </span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}

// ============================================

// 3️⃣ Add to your page
// File: pages/dashboard.tsx or app/page.tsx

import BlockchainFeed from '@/components/BlockchainFeed';

export default function Dashboard() {
  return (
    <div style={{ padding: '20px' }}>
      <h1>BlackBook Dashboard</h1>
      
      <div style={{ 
        display: 'grid', 
        gridTemplateColumns: '1fr 1fr', 
        gap: '20px',
        marginTop: '20px'
      }}>
        {/* Your existing components */}
        <div>
          {/* Wallet, Markets, etc. */}
        </div>

        {/* NEW: Blockchain Feed */}
        <div>
          <BlockchainFeed />
        </div>
      </div>
    </div>
  );
}

// ============================================
// 🎉 DONE! Your frontend is now connected to the blockchain feed!
// ============================================

// The feed will:
// ✅ Update every 3 seconds automatically
// ✅ Show all blockchain activities in real-time
// ✅ Display stats (activities, markets, live bets)
// ✅ Work with any React/Next.js app

// Test it:
// 1. Run your blockchain: cargo run
// 2. Run your frontend: npm run dev
// 3. Place a bet or connect a wallet
// 4. Watch the feed update in real-time! 🚀
