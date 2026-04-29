self.addEventListener('push', function(event) {
  console.log('Push event received:', event);

  const data = event.data ? event.data.json() : {};

  const options = {
    body: data.body || 'New Signal message. Tap to open inbox.',
    icon: '/icon-192.png',
    badge: '/icon-192.png',
    vibrate: [100, 50, 100],
    data: {
      url: data.url || '/'
    },
    actions: [
      { action: 'open', title: 'Open Inbox' }
    ]
  };

  event.waitUntil(
    self.registration.showNotification(data.title || 'Signal', options)
  );
});

self.addEventListener('notificationclick', function(event) {
  console.log('Notification click:', event);

  event.notification.close();

  event.waitUntil(
    clients.openWindow(event.notification.data.url || '/')
  );
});

self.addEventListener('message', function(event) {
  console.log('Service worker message:', event.data);
});
